// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! Integration tests of the client against a controlled station on loopback. The
//! station answers each procedure as §7 describes: a confirmation, then a
//! termination for the commands that have an execution.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use std::net::SocketAddr;
use std::time::Duration;

use igs_iec104::client::{Client, ClientConfig, Event, ReconnectPolicy};
use igs_iec104::{run_shared, Command, Delivery};
use igs_iec104_codec::asdu::{Asdu, Body, InformationObject, Objects};
use igs_iec104_codec::elements::{
    CounterFreeze, Dco, DoubleCommandState, Nva, Qcc, Qoc, Qoi, Qos, QualityFlags, Rco,
    RegulatingStep, Sco, ShortFloat, Siq, Sva,
};
use igs_iec104_codec::formats::Cp56Time2a;
use igs_iec104_codec::header::{
    cause, CauseOfTransmission, CommonAddress, InformationObjectAddress,
};
use igs_iec104_link::{LinkConfig, Role, TransferState};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc;
use tokio::time::timeout;

const STATION: u16 = 1;

fn time() -> Cp56Time2a {
    Cp56Time2a::new(1000, 30, 12, 1, 1, 1, 26).expect("in range")
}

fn address(value: u32) -> InformationObjectAddress {
    InformationObjectAddress::new(value).expect("in range")
}

fn config(address: SocketAddr) -> ClientConfig {
    let mut config = ClientConfig::new(address, CommonAddress::new(STATION));
    config.reconnect = ReconnectPolicy {
        initial: Duration::from_millis(50),
        max: Duration::from_millis(200),
    };
    config
}

/// A controlled station on loopback. When `drop_first` is set, its first connection
/// is closed at once, so that the client has to reconnect.
async fn serve(drop_first: bool) -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("binds");
    let address = listener.local_addr().expect("has an address");
    tokio::spawn(async move {
        let mut first = drop_first;
        while let Ok((stream, _)) = listener.accept().await {
            if std::mem::take(&mut first) {
                drop(stream);
                continue;
            }
            tokio::spawn(answer(stream));
        }
    });
    address
}

/// Answers the requests of one connection, until it ends.
async fn answer(stream: TcpStream) {
    let (commands, mut commands_rx) = mpsc::channel(32);
    let (deliveries_tx, mut deliveries) = mpsc::channel(32);
    tokio::spawn(async move {
        let config = LinkConfig::with_defaults(Role::Controlled);
        let _ = run_shared(stream, config, &mut commands_rx, &deliveries_tx).await;
    });
    while let Some(delivery) = deliveries.recv().await {
        if let Delivery::Asdu(request) = delivery {
            for reply in replies(&request) {
                if commands.send(Command::SendAsdu(reply)).await.is_err() {
                    return;
                }
            }
        }
    }
}

fn reply(request: &Asdu, code: u8, body: Body) -> Asdu {
    Asdu {
        cot: CauseOfTransmission::new(code).expect("valid cause"),
        common_address: request.common_address,
        body,
    }
}

/// The first object of a request, if it has one.
fn first_address<T>(objects: &Objects<T>) -> Option<InformationObjectAddress> {
    match objects {
        Objects::Individual(list) => list.first().map(|object| object.address),
        Objects::Sequence { address, .. } => Some(*address),
    }
}

/// The first value of a request, if it has one.
fn only<T>(objects: &Objects<T>) -> Option<&T> {
    match objects {
        Objects::Individual(list) => list.first().map(|object| &object.value),
        Objects::Sequence { values, .. } => values.first(),
    }
}

/// The select flag of a command, if the ASDU is a command.
fn select_flag(body: &Body) -> Option<bool> {
    match body {
        Body::C_SC_NA_1(o) => only(o).map(|c| c.qoc.select()),
        Body::C_SC_TA_1(o) => only(o).map(|c| c.value.qoc.select()),
        Body::C_DC_NA_1(o) => only(o).map(|c| c.qoc.select()),
        Body::C_DC_TA_1(o) => only(o).map(|c| c.value.qoc.select()),
        Body::C_RC_NA_1(o) => only(o).map(|c| c.qoc.select()),
        Body::C_RC_TA_1(o) => only(o).map(|c| c.value.qoc.select()),
        Body::C_SE_NA_1(o) => only(o).map(|(_, q)| q.select()),
        Body::C_SE_TA_1(o) => only(o).map(|t| t.value.1.select()),
        Body::C_SE_NB_1(o) => only(o).map(|(_, q)| q.select()),
        Body::C_SE_TB_1(o) => only(o).map(|t| t.value.1.select()),
        Body::C_SE_NC_1(o) => only(o).map(|(_, q)| q.select()),
        Body::C_SE_TC_1(o) => only(o).map(|t| t.value.1.select()),
        _ => None,
    }
}

/// The answers of the station to one request, as §7 gives them.
fn replies(request: &Asdu) -> Vec<Asdu> {
    let code = request.cot.cause();
    match &request.body {
        Body::C_IC_NA_1(_) | Body::C_CI_NA_1(_) if code == cause::ACTIVATION => vec![
            reply(
                request,
                cause::ACTIVATION_CONFIRMATION,
                request.body.clone(),
            ),
            reply(request, cause::ACTIVATION_TERMINATION, request.body.clone()),
        ],
        // The answer carries the time before the synchronization; this station echoes the request.
        Body::C_CS_NA_1(_) | Body::C_TS_TA_1(_) if code == cause::ACTIVATION => {
            vec![reply(
                request,
                cause::ACTIVATION_CONFIRMATION,
                request.body.clone(),
            )]
        }
        Body::C_RD_NA_1(objects) if code == cause::REQUEST => match first_address(objects) {
            Some(address) => vec![reply(
                request,
                cause::REQUEST,
                Body::M_SP_NA_1(Objects::Individual(vec![InformationObject {
                    address,
                    value: Siq {
                        on: true,
                        quality: QualityFlags::default(),
                    },
                }])),
            )],
            None => Vec::new(),
        },
        _ => match select_flag(&request.body) {
            Some(true) => vec![reply(
                request,
                cause::ACTIVATION_CONFIRMATION,
                request.body.clone(),
            )],
            Some(false) => vec![
                reply(
                    request,
                    cause::ACTIVATION_CONFIRMATION,
                    request.body.clone(),
                ),
                reply(request, cause::ACTIVATION_TERMINATION, request.body.clone()),
            ],
            None => Vec::new(),
        },
    }
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

/// An ASDU with the cause `code`, of a type that `kind` accepts.
fn answered(kind: fn(&Body) -> bool, code: u8) -> impl Fn(&Event) -> bool {
    move |event| {
        matches!(event, Event::Delivery(Delivery::Asdu(asdu))
            if asdu.cot.cause() == code && kind(&asdu.body))
    }
}

async fn start(client: &mut Client) {
    client.start_data_transfer().await.expect("the client runs");
    next_until(
        client,
        transfer(TransferState::Started),
        "the start confirmation",
    )
    .await;
}

#[tokio::test]
async fn the_client_reconnects_after_the_station_drops_the_connection() {
    let address = serve(true).await;
    let mut client = Client::connect(config(address));
    next_until(&mut client, connected, "the first connection").await;
    next_until(
        &mut client,
        |e| matches!(e, Event::Disconnected(_)),
        "the drop",
    )
    .await;
    next_until(&mut client, connected, "the second connection").await;
}

#[tokio::test]
async fn start_and_stop_follow_the_confirmations() {
    let mut client = Client::connect(config(serve(false).await));
    next_until(&mut client, connected, "the connection").await;
    start(&mut client).await;
    client.stop_data_transfer().await.expect("the client runs");
    next_until(
        &mut client,
        transfer(TransferState::Stopped),
        "the stop confirmation",
    )
    .await;
}

#[tokio::test]
async fn general_interrogation_is_confirmed_and_terminated() {
    let mut client = Client::connect(config(serve(false).await));
    next_until(&mut client, connected, "the connection").await;
    start(&mut client).await;
    client
        .interrogate(Qoi::STATION)
        .await
        .expect("the client runs");
    let is_interrogation = |body: &Body| matches!(body, Body::C_IC_NA_1(_));
    next_until(
        &mut client,
        answered(is_interrogation, cause::ACTIVATION_CONFIRMATION),
        "ACTCON",
    )
    .await;
    next_until(
        &mut client,
        answered(is_interrogation, cause::ACTIVATION_TERMINATION),
        "ACTTERM",
    )
    .await;
}

#[tokio::test]
async fn counter_interrogation_is_confirmed_and_terminated() {
    let mut client = Client::connect(config(serve(false).await));
    next_until(&mut client, connected, "the connection").await;
    start(&mut client).await;
    let request = Qcc::new(1, CounterFreeze::Read).expect("in range");
    client
        .counter_interrogation(request)
        .await
        .expect("the client runs");
    let is_counter = |body: &Body| matches!(body, Body::C_CI_NA_1(_));
    next_until(
        &mut client,
        answered(is_counter, cause::ACTIVATION_CONFIRMATION),
        "ACTCON",
    )
    .await;
    next_until(
        &mut client,
        answered(is_counter, cause::ACTIVATION_TERMINATION),
        "ACTTERM",
    )
    .await;
}

#[tokio::test]
async fn read_is_answered_with_the_object() {
    let mut client = Client::connect(config(serve(false).await));
    next_until(&mut client, connected, "the connection").await;
    start(&mut client).await;
    client.read(address(672)).await.expect("the client runs");
    let is_point = |body: &Body| matches!(body, Body::M_SP_NA_1(_));
    next_until(
        &mut client,
        answered(is_point, cause::REQUEST),
        "the answer to the read",
    )
    .await;
}

#[tokio::test]
async fn clock_synchronization_is_confirmed_as_an_activation() {
    let mut client = Client::connect(config(serve(false).await));
    next_until(&mut client, connected, "the connection").await;
    start(&mut client).await;
    client.clock_sync(time()).await.expect("the client runs");
    let is_clock = |body: &Body| matches!(body, Body::C_CS_NA_1(_));
    next_until(
        &mut client,
        answered(is_clock, cause::ACTIVATION_CONFIRMATION),
        "the clock answer",
    )
    .await;
}

#[tokio::test]
async fn test_command_is_confirmed() {
    let mut client = Client::connect(config(serve(false).await));
    next_until(&mut client, connected, "the connection").await;
    start(&mut client).await;
    client
        .test_command(0x55AA, time())
        .await
        .expect("the client runs");
    let is_test = |body: &Body| matches!(body, Body::C_TS_TA_1(_));
    next_until(
        &mut client,
        answered(is_test, cause::ACTIVATION_CONFIRMATION),
        "the test answer",
    )
    .await;
}

#[tokio::test]
async fn single_command_is_selected_then_executed() {
    let mut client = Client::connect(config(serve(false).await));
    next_until(&mut client, connected, "the connection").await;
    start(&mut client).await;
    let command = |select| Sco {
        on: true,
        qoc: Qoc::new(0, select).expect("in range"),
    };
    client
        .single_command(address(1), command(true), None)
        .await
        .expect("the client runs");
    let is_single = |body: &Body| matches!(body, Body::C_SC_NA_1(_));
    next_until(
        &mut client,
        answered(is_single, cause::ACTIVATION_CONFIRMATION),
        "the select answer",
    )
    .await;

    client
        .single_command(address(1), command(false), Some(time()))
        .await
        .expect("the client runs");
    let is_single_tagged = |body: &Body| matches!(body, Body::C_SC_TA_1(_));
    next_until(
        &mut client,
        answered(is_single_tagged, cause::ACTIVATION_CONFIRMATION),
        "ACTCON",
    )
    .await;
    next_until(
        &mut client,
        answered(is_single_tagged, cause::ACTIVATION_TERMINATION),
        "ACTTERM",
    )
    .await;
}

#[tokio::test]
async fn double_regulating_and_set_point_commands_are_confirmed() {
    let mut client = Client::connect(config(serve(false).await));
    next_until(&mut client, connected, "the connection").await;
    start(&mut client).await;

    let double = Dco {
        state: DoubleCommandState::On,
        qoc: Qoc::new(0, true).expect("in range"),
    };
    client
        .double_command(address(2), double, None)
        .await
        .expect("the client runs");
    let is_double = |body: &Body| matches!(body, Body::C_DC_NA_1(_));
    next_until(
        &mut client,
        answered(is_double, cause::ACTIVATION_CONFIRMATION),
        "the double answer",
    )
    .await;

    let step = Rco {
        step: RegulatingStep::Higher,
        qoc: Qoc::new(0, false).expect("in range"),
    };
    client
        .regulating_step(address(3), step, Some(time()))
        .await
        .expect("the client runs");
    let is_step = |body: &Body| matches!(body, Body::C_RC_TA_1(_));
    next_until(
        &mut client,
        answered(is_step, cause::ACTIVATION_TERMINATION),
        "the step termination",
    )
    .await;

    let qualifier = Qos::new(0, false).expect("in range");
    client
        .set_point_float(address(4), ShortFloat::from_f32(1.5), qualifier, None)
        .await
        .expect("the client runs");
    let is_float = |body: &Body| matches!(body, Body::C_SE_NC_1(_));
    next_until(
        &mut client,
        answered(is_float, cause::ACTIVATION_TERMINATION),
        "the set-point termination",
    )
    .await;

    client
        .set_point_normalized(address(5), Nva::from_raw(1000), qualifier, None)
        .await
        .expect("the client runs");
    let is_normalized = |body: &Body| matches!(body, Body::C_SE_NA_1(_));
    next_until(
        &mut client,
        answered(is_normalized, cause::ACTIVATION_TERMINATION),
        "the normalized termination",
    )
    .await;

    client
        .set_point_scaled(address(6), Sva::new(1000), qualifier, Some(time()))
        .await
        .expect("the client runs");
    let is_scaled = |body: &Body| matches!(body, Body::C_SE_TB_1(_));
    next_until(
        &mut client,
        answered(is_scaled, cause::ACTIVATION_TERMINATION),
        "the scaled termination",
    )
    .await;
}
