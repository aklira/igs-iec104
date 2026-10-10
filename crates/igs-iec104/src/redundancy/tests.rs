// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! Tests of the controlling station's redundancy group, over loopback. Each endpoint is a
//! controlled station of this crate; a relay in front of each one can be cut, which closes the
//! connection as a failed network would.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use igs_iec104_codec::asdu::{Asdu, Body, InformationObject, Objects};
use igs_iec104_codec::elements::{Qoi, Siq};
use igs_iec104_codec::header::{
    cause, CauseOfTransmission, CommonAddress, InformationObjectAddress,
};
use tokio::io::copy_bidirectional;
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;
use tokio::time::timeout;

use super::{RedundancyError, RedundancyEvent, RedundantClient};
use crate::client::{ClientConfig, Event, ReconnectPolicy};
use crate::process_image::PointValue;
use crate::server::{Handler, Operation, Refusal, Server, ServerConfig};
use crate::transport::Delivery;

const STATION: u16 = 45;

struct Accept;

impl Handler for Accept {
    fn select(&self, _operation: &Operation) -> Result<(), Refusal> {
        Ok(())
    }

    fn execute(&self, _operation: &Operation) -> Result<(), Refusal> {
        Ok(())
    }
}

/// A controlled station on loopback, with one single point whose value is `on`.
async fn station(on: bool) -> SocketAddr {
    let config = ServerConfig::new(
        "127.0.0.1:0".parse().expect("an address"),
        CommonAddress::new(STATION),
    );
    let server = Server::bind(config, Accept).await.expect("binds");
    let address = server.local_addr().expect("has an address");
    let handle = server.handle();
    let siq = Siq::decode(&[u8::from(on)]).expect("a single point");
    handle
        .add_point(
            InformationObjectAddress::new(672).expect("in range"),
            PointValue::Single(siq),
            false,
        )
        .expect("registered");
    tokio::spawn(server.run());
    address
}

/// A TCP relay in front of a station. Cutting it closes every connection through it.
struct Relay {
    address: SocketAddr,
    listener: JoinHandle<()>,
    connections: Arc<Mutex<Vec<JoinHandle<()>>>>,
}

async fn relay(target: SocketAddr) -> Relay {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("binds");
    let address = listener.local_addr().expect("has an address");
    let connections = Arc::new(Mutex::new(Vec::new()));
    let kept = Arc::clone(&connections);
    let task = tokio::spawn(async move {
        while let Ok((mut inbound, _)) = listener.accept().await {
            let connection = tokio::spawn(async move {
                if let Ok(mut outbound) = TcpStream::connect(target).await {
                    let _ = copy_bidirectional(&mut inbound, &mut outbound).await;
                }
            });
            kept.lock().expect("not poisoned").push(connection);
        }
    });
    Relay {
        address,
        listener: task,
        connections,
    }
}

impl Relay {
    fn cut(&self) {
        self.listener.abort();
        for connection in self.connections.lock().expect("not poisoned").drain(..) {
            connection.abort();
        }
    }
}

fn endpoint(address: SocketAddr) -> ClientConfig {
    let mut config = ClientConfig::new(address, CommonAddress::new(STATION));
    config.reconnect = ReconnectPolicy {
        initial: Duration::from_millis(20),
        max: Duration::from_millis(100),
    };
    config
}

/// Waits, for at most 10 s, for the first event `wanted` accepts.
async fn next_matching(
    group: &mut RedundantClient,
    mut wanted: impl FnMut(&RedundancyEvent) -> bool,
    what: &str,
) -> RedundancyEvent {
    timeout(Duration::from_secs(10), async {
        loop {
            let event = group.next_event().await.expect("the group runs");
            if wanted(&event) {
                return event;
            }
        }
    })
    .await
    .unwrap_or_else(|_| panic!("no {what} within 10 s"))
}

/// The change of the started connection, if `event` is one.
fn switched(event: &RedundancyEvent) -> Option<(Option<usize>, usize)> {
    match event {
        RedundancyEvent::Switched { from, to } => Some((*from, *to)),
        RedundancyEvent::Endpoint { .. } => None,
    }
}

/// An ASDU that endpoint `index` sent, with the cause `code`, of a type `kind` accepts.
fn answer_from(
    index: usize,
    kind: fn(&Body) -> bool,
    code: u8,
) -> impl Fn(&RedundancyEvent) -> bool {
    move |event| match event {
        RedundancyEvent::Endpoint {
            index: from,
            event: Event::Delivery(Delivery::Asdu(asdu)),
        } => *from == index && asdu.cot.cause() == code && kind(&asdu.body),
        _ => false,
    }
}

fn is_single(body: &Body) -> bool {
    matches!(body, Body::M_SP_NA_1(_))
}

/// The first switchover of a group: the endpoint it started, and the points its first station
/// interrogation brought (§10.3).
async fn first_start(group: &mut RedundantClient) -> usize {
    let (from, first) =
        switched(&next_matching(group, |e| switched(e).is_some(), "the first start").await)
            .expect("a switchover");
    assert_eq!(from, None);
    next_matching(
        group,
        answer_from(first, is_single, cause::INTERROGATED_STATION),
        "the points of the first start",
    )
    .await;
    first
}

#[tokio::test]
async fn the_first_endpoint_to_connect_is_started_and_the_other_waits_stopped() {
    let a = station(true).await;
    let b = station(true).await;
    let mut group =
        RedundantClient::connect(vec![endpoint(a), endpoint(b)]).expect("two endpoints");
    let first = first_start(&mut group).await;
    assert!(first < 2);

    // A station interrogation of the group goes to the started endpoint.
    group.interrogate(Qoi::STATION).await.expect("started");
    next_matching(
        &mut group,
        answer_from(first, is_single, cause::INTERROGATED_STATION),
        "the points of the request",
    )
    .await;
}

#[tokio::test]
async fn a_lost_started_connection_is_replaced_and_the_new_one_is_interrogated() {
    let a = station(true).await;
    let b = station(false).await;
    let relay_a = relay(a).await;
    let relay_b = relay(b).await;
    let mut group =
        RedundantClient::connect(vec![endpoint(relay_a.address), endpoint(relay_b.address)])
            .expect("two endpoints");
    let first = first_start(&mut group).await;
    let other = 1 - first;

    // The started connection is cut: the group starts the other one, and asks it for the
    // points, so that no change is missed (§10.5).
    let cut = if first == 0 { &relay_a } else { &relay_b };
    cut.cut();
    let (from, to) =
        switched(&next_matching(&mut group, |e| switched(e).is_some(), "the switchover").await)
            .expect("a switchover");
    assert_eq!((from, to), (Some(first), other));
    next_matching(
        &mut group,
        answer_from(other, is_single, cause::INTERROGATED_STATION),
        "the points after the switchover",
    )
    .await;
}

#[tokio::test]
async fn the_application_switches_over_by_hand() {
    let a = station(true).await;
    let b = station(true).await;
    let mut group =
        RedundantClient::connect(vec![endpoint(a), endpoint(b)]).expect("two endpoints");
    let first = first_start(&mut group).await;
    let other = 1 - first;

    group.switch_to(other).await.expect("connected");
    let (from, to) = switched(
        &next_matching(
            &mut group,
            |e| switched(e).is_some(),
            "the manual switchover",
        )
        .await,
    )
    .expect("a switchover");
    assert_eq!((from, to), (Some(first), other));
    next_matching(
        &mut group,
        answer_from(other, is_single, cause::INTERROGATED_STATION),
        "the points after the switchover",
    )
    .await;
}

#[tokio::test]
async fn an_empty_group_is_refused_and_a_send_without_a_started_connection_fails() {
    assert!(RedundantClient::connect(Vec::new()).is_err());
    // Nothing listens on this port: the endpoint never connects, so nothing is started.
    let nowhere: SocketAddr = "127.0.0.1:9".parse().expect("an address");
    let group = RedundantClient::connect(vec![endpoint(nowhere)]).expect("one endpoint");
    let asdu = Asdu {
        cot: CauseOfTransmission::new(cause::ACTIVATION).expect("a cause"),
        common_address: CommonAddress::new(STATION),
        body: Body::C_IC_NA_1(Objects::Individual(vec![InformationObject {
            address: InformationObjectAddress::new(0).expect("in range"),
            value: Qoi::STATION,
        }])),
    };
    assert_eq!(
        group.send_asdu(asdu).await,
        Err(RedundancyError::NoStartedConnection)
    );
}
