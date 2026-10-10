// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! Integration tests of the controlled station with the client, over loopback. Each test
//! runs one procedure of §7 from the controlling side and checks the answers it receives:
//! the causes, the P/N bit and the data, as the station sends them.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use std::net::SocketAddr;
use std::time::Duration;

use igs_iec104::client::{Client, ClientConfig, Event};
use igs_iec104::process_image::{PointValue, Update};
use igs_iec104::server::{Handler, Operation, Refusal, Server, ServerConfig, ServerHandle};
use igs_iec104::Delivery;
use igs_iec104_codec::asdu::{Asdu, Body, InformationObject, Objects};
use igs_iec104_codec::elements::{
    Bcr, CounterFreeze, Nva, Qcc, Qds, Qoc, Qoi, Qos, Sco, ShortFloat, Siq,
};
use igs_iec104_codec::formats::Cp56Time2a;
use igs_iec104_codec::header::{
    cause, CauseOfTransmission, CommonAddress, InformationObjectAddress,
};
use igs_iec104_link::TransferState;
use tokio::time::timeout;

const STATION: u16 = 45;
const SINGLE: u32 = 672;
const FLOAT: u32 = 984;
const COUNTER: u32 = 10;
const OTHER_COUNTER: u32 = 11;
const UNKNOWN: u32 = 999;
const REFUSED: u32 = 13;

fn time() -> Cp56Time2a {
    Cp56Time2a::new(1000, 30, 12, 1, 1, 1, 26).expect("in range")
}

fn quality() -> Qds {
    Qds::decode(&[0]).expect("a quality octet")
}

/// Refuses the addresses `UNKNOWN` (cause 47) and `REFUSED` (negative confirmation); accepts
/// every other command, and every clock synchronization.
struct Judge;

impl Judge {
    fn judge(operation: &Operation) -> Result<(), Refusal> {
        match operation.address.value() {
            UNKNOWN => Err(Refusal::UnknownAddress),
            REFUSED => Err(Refusal::Rejected),
            _ => Ok(()),
        }
    }
}

impl Handler for Judge {
    fn select(&self, operation: &Operation) -> Result<(), Refusal> {
        Self::judge(operation)
    }

    fn execute(&self, operation: &Operation) -> Result<(), Refusal> {
        Self::judge(operation)
    }
}

/// A station on loopback with a single point, a float and two counters, the first in group 2.
async fn station() -> (SocketAddr, ServerHandle) {
    let config = ServerConfig::new(
        "127.0.0.1:0".parse().expect("an address"),
        CommonAddress::new(STATION),
    );
    let server = Server::bind(config, Judge).await.expect("binds");
    let address = server.local_addr().expect("has an address");
    let handle = server.handle();
    handle
        .add_point(
            address_of(SINGLE),
            PointValue::Single(Siq::decode(&[0x00]).expect("off")),
            false,
        )
        .expect("registered");
    handle
        .add_point(
            address_of(FLOAT),
            PointValue::Float(ShortFloat::from_f32(1.5), quality()),
            false,
        )
        .expect("registered");
    let counter = PointValue::Counter(Bcr::decode(&[0; 5]).expect("a counter"));
    handle
        .add_point(address_of(COUNTER), counter, false)
        .expect("registered");
    handle
        .add_point(address_of(OTHER_COUNTER), counter, false)
        .expect("registered");
    handle.set_group(address_of(SINGLE), 1).expect("a group");
    handle.set_group(address_of(FLOAT), 1).expect("a group");
    handle.set_group(address_of(COUNTER), 2).expect("a group");
    tokio::spawn(server.run());
    (address, handle)
}

fn address_of(value: u32) -> InformationObjectAddress {
    InformationObjectAddress::new(value).expect("in range")
}

fn client(address: SocketAddr, common: u16) -> Client {
    Client::connect(ClientConfig::new(address, CommonAddress::new(common)))
}

/// Waits, for at most 10 s, for the first event `wanted` accepts.
async fn next_until(
    client: &mut Client,
    mut wanted: impl FnMut(&Event) -> bool,
    what: &str,
) -> Event {
    timeout(Duration::from_secs(10), async {
        loop {
            let event = client.next_event().await.expect("the client runs");
            if wanted(&event) {
                return event;
            }
        }
    })
    .await
    .unwrap_or_else(|_| panic!("no {what} within 10 s"))
}

fn connected(event: &Event) -> bool {
    matches!(event, Event::Connected)
}

fn transfer(state: TransferState) -> impl Fn(&Event) -> bool {
    move |event| matches!(event, Event::Delivery(Delivery::Transfer(s)) if *s == state)
}

/// An ASDU of a type `kind` accepts, with the cause `code` and the P/N bit `negative`.
fn answer(kind: fn(&Body) -> bool, code: u8, negative: bool) -> impl Fn(&Event) -> bool {
    move |event| {
        matches!(event, Event::Delivery(Delivery::Asdu(asdu))
            if asdu.cot.cause() == code && asdu.cot.negative == negative && kind(&asdu.body))
    }
}

async fn started(address: SocketAddr, common: u16) -> Client {
    let mut client = client(address, common);
    next_until(&mut client, connected, "the connection").await;
    client.start_data_transfer().await.expect("the client runs");
    next_until(
        &mut client,
        transfer(TransferState::Started),
        "the start confirmation",
    )
    .await;
    client
}

fn is_single(body: &Body) -> bool {
    matches!(body, Body::M_SP_NA_1(_))
}

fn is_float(body: &Body) -> bool {
    matches!(body, Body::M_ME_NC_1(_))
}

fn is_counter(body: &Body) -> bool {
    matches!(body, Body::M_IT_NA_1(_))
}

fn is_interrogation(body: &Body) -> bool {
    matches!(body, Body::C_IC_NA_1(_))
}

fn is_counter_interrogation(body: &Body) -> bool {
    matches!(body, Body::C_CI_NA_1(_))
}

fn is_read(body: &Body) -> bool {
    matches!(body, Body::C_RD_NA_1(_))
}

fn is_clock(body: &Body) -> bool {
    matches!(body, Body::C_CS_NA_1(_))
}

fn is_test(body: &Body) -> bool {
    matches!(body, Body::C_TS_TA_1(_))
}

fn is_single_command(body: &Body) -> bool {
    matches!(body, Body::C_SC_NA_1(_) | Body::C_SC_TA_1(_))
}

fn is_end_of_initialization(body: &Body) -> bool {
    matches!(body, Body::M_EI_NA_1(_))
}

#[tokio::test]
async fn the_station_announces_its_end_of_initialization_when_the_transfer_starts() {
    let (address, _) = station().await;
    let mut client = started(address, STATION).await;
    next_until(
        &mut client,
        answer(is_end_of_initialization, cause::INITIALIZED, false),
        "the end of initialization",
    )
    .await;
}

#[tokio::test]
async fn a_general_interrogation_is_confirmed_answered_and_terminated() {
    let (address, _) = station().await;
    let mut client = started(address, STATION).await;
    client
        .interrogate(Qoi::STATION)
        .await
        .expect("the client runs");
    next_until(
        &mut client,
        answer(is_interrogation, cause::ACTIVATION_CONFIRMATION, false),
        "the confirmation",
    )
    .await;
    next_until(
        &mut client,
        answer(is_single, cause::INTERROGATED_STATION, false),
        "the single point",
    )
    .await;
    next_until(
        &mut client,
        answer(is_float, cause::INTERROGATED_STATION, false),
        "the float",
    )
    .await;
    next_until(
        &mut client,
        answer(is_interrogation, cause::ACTIVATION_TERMINATION, false),
        "the termination",
    )
    .await;
}

#[tokio::test]
async fn a_group_interrogation_answers_only_the_points_of_its_group() {
    let (address, _) = station().await;
    let mut client = started(address, STATION).await;
    client
        .interrogate(Qoi::group(1).expect("group 1"))
        .await
        .expect("the client runs");
    next_until(
        &mut client,
        answer(is_interrogation, cause::ACTIVATION_CONFIRMATION, false),
        "the confirmation",
    )
    .await;
    next_until(
        &mut client,
        answer(is_single, cause::INTERROGATED_GROUP_1, false),
        "the single point of group 1",
    )
    .await;
    next_until(
        &mut client,
        answer(is_float, cause::INTERROGATED_GROUP_1, false),
        "the float of group 1",
    )
    .await;
    next_until(
        &mut client,
        answer(is_interrogation, cause::ACTIVATION_TERMINATION, false),
        "the termination",
    )
    .await;
}

#[tokio::test]
async fn a_counter_interrogation_answers_the_counters() {
    let (address, _) = station().await;
    let mut client = started(address, STATION).await;
    let general = Qcc::new(5, CounterFreeze::Read).expect("a request");
    client
        .counter_interrogation(general)
        .await
        .expect("the client runs");
    next_until(
        &mut client,
        answer(
            is_counter_interrogation,
            cause::ACTIVATION_CONFIRMATION,
            false,
        ),
        "the confirmation",
    )
    .await;
    next_until(
        &mut client,
        answer(is_counter, cause::COUNTER_GENERAL, false),
        "the first counter",
    )
    .await;
    next_until(
        &mut client,
        answer(is_counter, cause::COUNTER_GENERAL, false),
        "the second counter",
    )
    .await;
    next_until(
        &mut client,
        answer(
            is_counter_interrogation,
            cause::ACTIVATION_TERMINATION,
            false,
        ),
        "the termination",
    )
    .await;
}

#[tokio::test]
async fn a_read_is_answered_with_the_point_and_an_unknown_address_is_refused() {
    let (address, _) = station().await;
    let mut client = started(address, STATION).await;
    client
        .read(address_of(SINGLE))
        .await
        .expect("the client runs");
    next_until(
        &mut client,
        answer(is_single, cause::REQUEST, false),
        "the answer to the read",
    )
    .await;

    client
        .read(address_of(UNKNOWN))
        .await
        .expect("the client runs");
    next_until(
        &mut client,
        answer(is_read, cause::UNKNOWN_INFO_OBJECT_ADDRESS, true),
        "the refusal of the read",
    )
    .await;
}

#[tokio::test]
async fn a_clock_synchronization_is_confirmed_with_the_time_of_the_station() {
    let (address, _) = station().await;
    let mut client = started(address, STATION).await;
    client.clock_sync(time()).await.expect("the client runs");
    next_until(
        &mut client,
        answer(is_clock, cause::ACTIVATION_CONFIRMATION, false),
        "the clock confirmation",
    )
    .await;
}

#[tokio::test]
async fn a_test_command_is_confirmed() {
    let (address, _) = station().await;
    let mut client = started(address, STATION).await;
    client
        .test_command(0x55AA, time())
        .await
        .expect("the client runs");
    next_until(
        &mut client,
        answer(is_test, cause::ACTIVATION_CONFIRMATION, false),
        "the test confirmation",
    )
    .await;
}

#[tokio::test]
async fn a_single_command_is_selected_then_executed_with_its_time_tag() {
    let (address, _) = station().await;
    let mut client = started(address, STATION).await;
    let command = |select| Sco {
        on: true,
        qoc: Qoc::new(0, select).expect("in range"),
    };
    client
        .single_command(address_of(SINGLE), command(true), None)
        .await
        .expect("the client runs");
    next_until(
        &mut client,
        answer(is_single_command, cause::ACTIVATION_CONFIRMATION, false),
        "the select confirmation",
    )
    .await;

    // The execution is of the same type as the selection: the time-tagged form on both.
    client
        .single_command(address_of(SINGLE), command(true), Some(time()))
        .await
        .expect("the client runs");
    next_until(
        &mut client,
        answer(is_single_command, cause::ACTIVATION_CONFIRMATION, false),
        "the second select confirmation",
    )
    .await;

    client
        .single_command(address_of(SINGLE), command(false), Some(time()))
        .await
        .expect("the client runs");
    next_until(
        &mut client,
        answer(is_single_command, cause::ACTIVATION_CONFIRMATION, false),
        "the execution confirmation",
    )
    .await;
    next_until(
        &mut client,
        answer(is_single_command, cause::ACTIVATION_TERMINATION, false),
        "the execution termination",
    )
    .await;
}

#[tokio::test]
async fn a_direct_execution_without_a_selection_is_refused() {
    let (address, _) = station().await;
    let mut client = started(address, STATION).await;
    let execute = Sco {
        on: false,
        qoc: Qoc::new(0, false).expect("in range"),
    };
    client
        .single_command(address_of(SINGLE), execute, None)
        .await
        .expect("the client runs");
    next_until(
        &mut client,
        answer(is_single_command, cause::ACTIVATION_CONFIRMATION, true),
        "the refusal",
    )
    .await;
}

#[tokio::test]
async fn a_command_to_an_unknown_address_is_mirrored_with_cause_47() {
    let (address, _) = station().await;
    let mut client = started(address, STATION).await;
    let select = Sco {
        on: true,
        qoc: Qoc::new(0, true).expect("in range"),
    };
    client
        .single_command(address_of(UNKNOWN), select, None)
        .await
        .expect("the client runs");
    next_until(
        &mut client,
        answer(is_single_command, cause::UNKNOWN_INFO_OBJECT_ADDRESS, true),
        "the mirror of the command",
    )
    .await;
}

#[tokio::test]
async fn a_command_the_application_refuses_is_a_negative_confirmation() {
    let (address, _) = station().await;
    let mut client = started(address, STATION).await;
    let select = Sco {
        on: true,
        qoc: Qoc::new(0, true).expect("in range"),
    };
    client
        .single_command(address_of(REFUSED), select, None)
        .await
        .expect("the client runs");
    next_until(
        &mut client,
        answer(is_single_command, cause::ACTIVATION_CONFIRMATION, true),
        "the refusal",
    )
    .await;
}

#[tokio::test]
async fn a_request_for_another_station_is_mirrored_with_cause_46() {
    let (address, _) = station().await;
    let mut client = started(address, STATION + 1).await;
    client
        .interrogate(Qoi::STATION)
        .await
        .expect("the client runs");
    next_until(
        &mut client,
        answer(is_interrogation, cause::UNKNOWN_COMMON_ADDRESS, true),
        "the mirror of the interrogation",
    )
    .await;
}

#[tokio::test]
async fn a_set_point_command_is_confirmed_and_terminated() {
    let (address, _) = station().await;
    let mut client = started(address, STATION).await;
    let qualifier = Qos::new(0, true).expect("in range");
    client
        .set_point_normalized(address_of(FLOAT), Nva::from_raw(3), qualifier, None)
        .await
        .expect("the client runs");
    let is_set_point = |body: &Body| matches!(body, Body::C_SE_NA_1(_));
    next_until(
        &mut client,
        answer(is_set_point, cause::ACTIVATION_CONFIRMATION, false),
        "the select confirmation",
    )
    .await;
}

#[tokio::test]
async fn a_message_the_station_cannot_place_is_mirrored_with_cause_44() {
    let (address, _) = station().await;
    let mut client = started(address, STATION).await;
    // A monitoring type is not sent by the controlling station: unknown type identification.
    let monitor = Asdu {
        cot: CauseOfTransmission::new(cause::ACTIVATION).expect("a cause"),
        common_address: CommonAddress::new(STATION),
        body: Body::M_SP_NA_1(Objects::Individual(vec![InformationObject {
            address: address_of(SINGLE),
            value: Siq::decode(&[0]).expect("a value"),
        }])),
    };
    client.send_asdu(monitor).await.expect("the client runs");
    next_until(
        &mut client,
        answer(is_single, cause::UNKNOWN_TYPE_ID, true),
        "the mirror of the message",
    )
    .await;
}

#[tokio::test]
async fn a_spontaneous_event_reaches_only_the_connections_whose_transfer_started() {
    let (address, handle) = station().await;
    let mut stopped = client(address, STATION);
    next_until(&mut stopped, connected, "the connection").await;
    let value = PointValue::Single(Siq::decode(&[0x01]).expect("on"));
    assert_eq!(
        handle.update(address_of(SINGLE), value, time()),
        Ok(Update::Queued)
    );
    let quiet = timeout(
        Duration::from_millis(300),
        next_until(&mut stopped, answer(is_single, 3, false), "an event"),
    )
    .await;
    assert!(
        quiet.is_err(),
        "no event is sent while the transfer is stopped"
    );

    let mut started = started(address, STATION).await;
    let off = PointValue::Single(Siq::decode(&[0x00]).expect("off"));
    handle
        .update(address_of(SINGLE), off, time())
        .expect("changed");
    next_until(
        &mut started,
        answer(is_single, cause::SPONTANEOUS, false),
        "the spontaneous event",
    )
    .await;
}

#[tokio::test]
async fn a_second_started_connection_supersedes_the_first_and_takes_the_events() {
    // Only one connection of a redundancy group carries data (§10.2): starting another one
    // closes the first (§10.7).
    let (address, handle) = station().await;
    let mut first = started(address, STATION).await;
    let mut second = started(address, STATION).await;
    next_until(
        &mut first,
        |event| matches!(event, Event::Disconnected(_)),
        "the close of the first connection",
    )
    .await;
    let value = PointValue::Float(ShortFloat::from_f32(2.5), quality());
    handle
        .update(address_of(FLOAT), value, time())
        .expect("changed");
    next_until(
        &mut second,
        answer(is_float, cause::SPONTANEOUS, false),
        "the event on the second connection",
    )
    .await;
}

#[tokio::test]
async fn the_station_keeps_the_value_it_reports_in_interrogations() {
    let (address, handle) = station().await;
    let value = PointValue::Float(ShortFloat::from_f32(9.5), quality());
    handle
        .update(address_of(FLOAT), value, time())
        .expect("changed");
    assert_eq!(handle.value(address_of(FLOAT)), Some(value));
    let mut client = started(address, STATION).await;
    client
        .interrogate(Qoi::STATION)
        .await
        .expect("the client runs");
    let event = next_until(
        &mut client,
        answer(is_float, cause::INTERROGATED_STATION, false),
        "the float",
    )
    .await;
    match event {
        Event::Delivery(Delivery::Asdu(Asdu {
            body: Body::M_ME_NC_1(objects),
            ..
        })) => {
            assert_eq!(
                objects,
                Objects::Individual(vec![InformationObject {
                    address: address_of(FLOAT),
                    value: (ShortFloat::from_f32(9.5), quality()),
                }])
            );
        }
        other => panic!("expected the float, got {other:?}"),
    }
}
