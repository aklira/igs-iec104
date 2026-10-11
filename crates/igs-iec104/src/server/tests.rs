// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! Switchover tests of the redundancy group, with a fake clock. The connections run in memory
//! (duplex streams) with the server's own code, so the tests need no socket. Timers are set far
//! beyond the test, so that the paused clock moves only for the short waits of `settle`.

use std::collections::{BTreeMap, VecDeque};
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use igs_iec104_codec::apci::{Apdu, SequenceNumber, UnnumberedFunction};
use igs_iec104_codec::asdu::{Asdu, Body, Objects};
use igs_iec104_codec::elements::{Coi, Qds, Qoi, ShortFloat};
use igs_iec104_codec::formats::Cp56Time2a;
use igs_iec104_codec::header::{cause, CommonAddress, InformationObjectAddress};
use igs_iec104_link::{LinkConfig, Parameters, Role, TransferState};
use tokio::io::{duplex, AsyncReadExt, AsyncWriteExt, DuplexStream};
use tokio::sync::{mpsc, Notify};
use tokio::time::{sleep, timeout};

use super::respond::{Handler, Operation, Refusal};
use super::{
    close_connections, serve, AbortOnDrop, ClosedReport, ConnectionEnd, EventSink, Group, Process,
    ServerEvent, ServerHandle, Station,
};
use crate::procedures;
use crate::process_image::{PointValue, ProcessImage};
use crate::transport::{run, Command, Delivery};

const STATION: u16 = 45;
const POINT: u32 = 672;

/// Accepts every operation; the switchover does not depend on the application.
struct Accept;

impl Handler for Accept {
    fn select(&self, _operation: &Operation) -> Result<(), Refusal> {
        Ok(())
    }

    fn execute(&self, _operation: &Operation) -> Result<(), Refusal> {
        Ok(())
    }
}

/// The parameters of every connection: the timers are far beyond the tests.
fn parameters() -> Parameters {
    Parameters {
        t1: Duration::from_secs(255),
        t3: Duration::from_secs(3600),
        ..Parameters::default()
    }
}

/// A station with one float point, and its handle to publish the changes of the point.
fn station() -> (Arc<Station<Accept>>, ServerHandle) {
    let process = Arc::new(Process {
        group: Mutex::new(Group {
            image: ProcessImage::new(NonZeroUsize::new(64).expect("a capacity")).expect("an image"),
            members: BTreeMap::new(),
            started: None,
            carry: VecDeque::new(),
            next_id: 0,
        }),
        queued: Notify::new(),
    });
    let common_address = CommonAddress::new(STATION);
    let station = Arc::new(Station {
        process: Arc::clone(&process),
        handler: Accept,
        link: LinkConfig::new(Role::Controlled, parameters()).expect("valid parameters"),
        common_address,
        initialization: Coi::new(0, false).expect("a cause"),
        selection_timeout: Duration::from_secs(30),
        stop: tokio::sync::watch::Sender::new(false),
        events: None,
    });
    let handle = ServerHandle {
        common_address,
        process,
    };
    handle
        .add_point(
            address(POINT),
            PointValue::Float(ShortFloat::from_f32(0.0), quality()),
            false,
        )
        .expect("registered");
    (station, handle)
}

fn address(value: u32) -> InformationObjectAddress {
    InformationObjectAddress::new(value).expect("in range")
}

fn quality() -> Qds {
    Qds::decode(&[0]).expect("a quality octet")
}

fn time() -> Cp56Time2a {
    Cp56Time2a::new(1000, 30, 12, 1, 1, 1, 26).expect("in range")
}

/// Changes the point to the value `n`, so that each call is a change of the image.
fn publish(handle: &ServerHandle, n: u8) {
    let value = PointValue::Float(ShortFloat::from_f32(f32::from(n)), quality());
    handle
        .update(address(POINT), value, time())
        .expect("changed");
}

/// Lets the connections run until they are idle, on the paused clock.
async fn settle() {
    sleep(Duration::from_millis(10)).await;
}

/// A connection the test does not read: its frames are never acknowledged, so they stay in
/// flight. The station starts it by a STARTDT act written by the test.
async fn silent_connection(station: &Arc<Station<Accept>>) -> DuplexStream {
    let (server_side, peer) = duplex(1 << 16);
    tokio::spawn(serve(
        server_side,
        Arc::clone(station),
        std::net::SocketAddr::from(([127, 0, 0, 1], 0)),
    ));
    peer
}

async fn start_silent(peer: &mut DuplexStream) {
    let frame = Apdu::Unnumbered(UnnumberedFunction::StartDtAct)
        .to_vec()
        .expect("the act encodes");
    peer.write_all(&frame).await.expect("the station reads");
}

/// A controlling station: a transport that acknowledges what it receives, and a queue of
/// what it received.
struct Controller {
    commands: mpsc::Sender<Command>,
    deliveries: mpsc::Receiver<Delivery>,
}

fn controlling_link() -> LinkConfig {
    LinkConfig::new(Role::Controlling, parameters()).expect("valid parameters")
}

fn controller(station: &Arc<Station<Accept>>) -> Controller {
    let (server_side, peer) = duplex(1 << 16);
    tokio::spawn(serve(
        server_side,
        Arc::clone(station),
        std::net::SocketAddr::from(([127, 0, 0, 1], 0)),
    ));
    let (commands, commands_rx) = mpsc::channel(32);
    let (deliveries_tx, deliveries) = mpsc::channel(32);
    tokio::spawn(async move {
        let _ = run(peer, controlling_link(), commands_rx, deliveries_tx).await;
    });
    Controller {
        commands,
        deliveries,
    }
}

impl Controller {
    async fn start(&self) {
        self.commands
            .send(Command::StartDt)
            .await
            .expect("the transport runs");
    }

    async fn next(&mut self) -> Option<Delivery> {
        timeout(Duration::from_secs(5), self.deliveries.recv())
            .await
            .expect("a delivery within 5 s")
    }

    async fn wait_started(&mut self) {
        loop {
            match self.next().await {
                Some(Delivery::Transfer(TransferState::Started)) => return,
                Some(_) => {}
                None => panic!("the connection ended before it started"),
            }
        }
    }

    /// The value of the next float the station sends.
    async fn next_value(&mut self) -> f32 {
        loop {
            match self.next().await {
                Some(Delivery::Asdu(Asdu {
                    body: Body::M_ME_NC_1(Objects::Individual(items)),
                    ..
                })) => {
                    let item = items.first().expect("one object");
                    return item.value.0.as_f32();
                }
                Some(_) => {}
                None => panic!("the connection ended before the next value"),
            }
        }
    }

    /// Waits until the transport ends, which is when the station closes the connection.
    async fn wait_closed(&mut self) {
        while self.next().await.is_some() {}
    }

    /// The causes of the ASDUs received up to the float `value`, which must come, and that float
    /// included.
    async fn causes_before(&mut self, value: f32) -> Vec<u8> {
        let mut causes = Vec::new();
        loop {
            match self.next().await {
                Some(Delivery::Asdu(asdu)) => {
                    let arrived = matches!(
                        &asdu.body,
                        Body::M_ME_NC_1(Objects::Individual(items))
                            if items.first().is_some_and(|item| item.value.0.as_f32() == value)
                    );
                    causes.push(asdu.cot.cause());
                    if arrived {
                        return causes;
                    }
                }
                Some(_) => {}
                None => panic!("the connection ended before the value {value}"),
            }
        }
    }
}

#[tokio::test(start_paused = true)]
async fn a_switchover_without_in_flight_frames_hands_over_only_the_new_events() {
    let (station, handle) = station();
    let mut first = controller(&station);
    first.start().await;
    first.wait_started().await;
    for n in 1u8..=3 {
        publish(&handle, n);
    }
    for n in 1u8..=3 {
        assert_eq!(first.next_value().await, f32::from(n));
    }
    // The controlling station acknowledges after t2 (§5.5): then the three frames are not in
    // flight any more, and the switchover has nothing to send again.
    sleep(Duration::from_secs(11)).await;

    let mut second = controller(&station);
    second.start().await;
    second.wait_started().await;
    first.wait_closed().await;

    publish(&handle, 4);
    publish(&handle, 5);
    assert_eq!(second.next_value().await, 4.0);
    assert_eq!(second.next_value().await, 5.0);
}

#[tokio::test(start_paused = true)]
async fn a_switchover_with_in_flight_frames_sends_them_again_on_the_new_connection() {
    let (station, handle) = station();
    let mut first = silent_connection(&station).await;
    start_silent(&mut first).await;
    settle().await;
    for n in 1u8..=5 {
        publish(&handle, n);
    }
    settle().await;

    let mut second = controller(&station);
    second.start().await;
    second.wait_started().await;
    // The five frames of the first connection were never acknowledged: they come again on
    // the second, once each, in their order (§10.6).
    for n in 1u8..=5 {
        assert_eq!(second.next_value().await, f32::from(n));
    }
    publish(&handle, 6);
    publish(&handle, 7);
    assert_eq!(second.next_value().await, 6.0);
    assert_eq!(second.next_value().await, 7.0);
}

#[tokio::test(start_paused = true)]
async fn the_answers_of_a_connection_are_not_sent_again_on_the_next_one() {
    // The first connection answers a general interrogation, and an event is published. The peer
    // acknowledges none of them. The event comes again on the second connection; the answers do
    // not, and neither does the end of initialization of the first connection.
    let (station, handle) = station();
    let mut first = silent_connection(&station).await;
    start_silent(&mut first).await;
    settle().await;
    let request = procedures::interrogation(CommonAddress::new(STATION), Qoi::STATION)
        .expect("a general interrogation");
    let frame = Apdu::Information {
        send: SequenceNumber::ZERO,
        receive: SequenceNumber::ZERO,
        asdu: request,
    }
    .to_vec()
    .expect("the request encodes");
    first.write_all(&frame).await.expect("the station reads");
    settle().await;
    publish(&handle, 3);
    settle().await;

    let mut second = controller(&station);
    second.start().await;
    second.wait_started().await;
    // Only the initialization of the second connection (cause 4), then the event (cause 3).
    assert_eq!(
        second.causes_before(3.0).await,
        vec![cause::INITIALIZED, cause::SPONTANEOUS]
    );
}

#[tokio::test(start_paused = true)]
async fn a_full_window_loses_no_event_across_a_switchover_and_keeps_their_order() {
    // Thirty events against a window of twelve: the first connection sends twelve, holds
    // the others, and none is acknowledged. All thirty reach the new connection in order.
    let (station, handle) = station();
    let mut first = silent_connection(&station).await;
    start_silent(&mut first).await;
    settle().await;
    for n in 1u8..=30 {
        publish(&handle, n);
    }
    settle().await;

    let mut second = controller(&station);
    second.start().await;
    second.wait_started().await;
    for n in 1u8..=30 {
        assert_eq!(second.next_value().await, f32::from(n), "event {n}");
    }
}

#[tokio::test(start_paused = true)]
async fn events_raised_while_no_connection_is_started_wait_for_the_next_one() {
    let (station, handle) = station();
    for n in 1u8..=3 {
        publish(&handle, n);
    }
    settle().await;

    let mut only = controller(&station);
    only.start().await;
    only.wait_started().await;
    for n in 1u8..=3 {
        assert_eq!(only.next_value().await, f32::from(n));
    }
}

#[tokio::test(start_paused = true)]
async fn a_stopped_connection_is_left_open_by_a_switchover() {
    let (station, _handle) = station();
    let mut stopped = silent_connection(&station).await;
    let mut starter = controller(&station);
    starter.start().await;
    starter.wait_started().await;
    settle().await;

    let mut byte = [0u8; 1];
    let read = timeout(Duration::from_secs(1), stopped.read(&mut byte)).await;
    assert!(read.is_err(), "the stopped connection is not closed");
}

#[tokio::test(start_paused = true)]
async fn the_connections_still_open_after_the_grace_period_are_closed_by_force() {
    let mut connections = tokio::task::JoinSet::new();
    // One connection closes within the grace period, the other never does.
    connections.spawn(sleep(Duration::from_millis(10)));
    connections.spawn(std::future::pending::<()>());
    let began = tokio::time::Instant::now();
    let forced = close_connections(&mut connections, Duration::from_secs(1)).await;
    assert_eq!(forced, 1);
    assert!(connections.is_empty());
    assert_eq!(began.elapsed(), Duration::from_secs(1));
}

#[tokio::test(start_paused = true)]
async fn connections_that_close_in_time_are_not_forced() {
    let mut connections = tokio::task::JoinSet::new();
    connections.spawn(sleep(Duration::from_millis(10)));
    connections.spawn(sleep(Duration::from_millis(20)));
    let began = tokio::time::Instant::now();
    assert_eq!(
        close_connections(&mut connections, Duration::from_secs(1)).await,
        0
    );
    assert_eq!(began.elapsed(), Duration::from_millis(20));
}

#[tokio::test]
async fn a_transport_is_aborted_with_its_connection() {
    // The transport task of a connection holds the stream; dropping its guard, as a connection
    // aborted by force does, ends the task and so drops the stream.
    let (held, released) = tokio::sync::oneshot::channel::<()>();
    let guard = AbortOnDrop(tokio::spawn(async move {
        let _held = held;
        std::future::pending::<()>().await;
    }));
    drop(guard);
    assert!(released.await.is_err());
}

#[test]
fn a_connection_dropped_without_an_end_is_reported_as_closed_by_force() {
    let (sender, mut receiver) = mpsc::channel(4);
    let dropped = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let sink = EventSink {
        events: sender,
        dropped: Arc::clone(&dropped),
    };
    drop(ClosedReport {
        events: Some(sink.clone()),
        id: 7,
        end: None,
    });
    assert!(matches!(
        receiver.try_recv(),
        Ok(ServerEvent::Closed {
            id: 7,
            end: ConnectionEnd::Forced
        })
    ));
    drop(ClosedReport {
        events: Some(sink),
        id: 8,
        end: Some(ConnectionEnd::Station),
    });
    assert!(matches!(
        receiver.try_recv(),
        Ok(ServerEvent::Closed {
            id: 8,
            end: ConnectionEnd::Station
        })
    ));
}
