// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! The demo station `igs104-server`, run as a process and driven by the client library over
//! loopback: the interrogation answers its points, a command is confirmed at a command point and
//! refused elsewhere, and a simulated change reaches the controlling station.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use std::net::{SocketAddr, TcpListener};
use std::process::{Child, Command as Process, Stdio};
use std::time::Duration;

use igs_iec104::client::{Client, ClientConfig, Event};
use igs_iec104::Delivery;
use igs_iec104_codec::asdu::Body;
use igs_iec104_codec::elements::{Qoc, Qoi, Sco};
use igs_iec104_codec::header::{cause, CommonAddress, InformationObjectAddress};
use igs_iec104_link::TransferState;
use tokio::time::{sleep, timeout};

const STATION: u16 = 1;

/// A port nothing listens on, found by binding and releasing it.
fn free_port() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").expect("binds");
    let address = listener.local_addr().expect("has an address");
    drop(listener);
    address
}

/// The demo station as a child process, stopped when the guard is dropped.
struct Station {
    child: Child,
    address: SocketAddr,
}

impl Drop for Station {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Starts the station with `extra` options, after its address, its common address and its duration.
fn start_station_with(seconds: u64, extra: &[&str]) -> Station {
    let address = free_port();
    let child = Process::new(env!("CARGO_BIN_EXE_igs104-server"))
        .args([
            "--bind",
            &address.to_string(),
            "-c",
            &STATION.to_string(),
            "--for",
            &seconds.to_string(),
        ])
        .args(extra)
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("the server starts");
    Station { child, address }
}

fn start_station(seconds: u64) -> Station {
    let address = free_port();
    let child = Process::new(env!("CARGO_BIN_EXE_igs104-server"))
        .args([
            "--bind",
            &address.to_string(),
            "-c",
            &STATION.to_string(),
            "--for",
            &seconds.to_string(),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("the server starts");
    Station { child, address }
}

/// Connects to the station, once it listens, and starts the data transfer.
async fn connect(address: SocketAddr) -> Client {
    let mut client = Client::connect(ClientConfig::new(address, CommonAddress::new(STATION)));
    timeout(Duration::from_secs(10), async {
        loop {
            if let Event::Connected = client.next_event().await.expect("the client runs") {
                return;
            }
        }
    })
    .await
    .expect("the station accepts the connection");
    client.start_data_transfer().await.expect("the client runs");
    next_until(
        &mut client,
        |e| {
            matches!(
                e,
                Event::Delivery(Delivery::Transfer(TransferState::Started))
            )
        },
        "the start",
    )
    .await;
    client
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

/// An ASDU with the cause `code` and the P/N bit `negative`, of a type `kind` accepts.
fn answer(kind: fn(&Body) -> bool, code: u8, negative: bool) -> impl Fn(&Event) -> bool {
    move |event| {
        matches!(event, Event::Delivery(Delivery::Asdu(asdu))
            if asdu.cot.cause() == code && asdu.cot.negative == negative && kind(&asdu.body))
    }
}

fn is_single(body: &Body) -> bool {
    matches!(body, Body::M_SP_NA_1(_))
}

fn is_single_stamped(body: &Body) -> bool {
    matches!(body, Body::M_SP_TB_1(_))
}

fn is_single_command(body: &Body) -> bool {
    matches!(body, Body::C_SC_NA_1(_))
}

fn address(value: u32) -> InformationObjectAddress {
    InformationObjectAddress::new(value).expect("in range")
}

#[tokio::test]
async fn the_station_answers_an_interrogation_with_its_points() {
    let station = start_station(20);
    let mut client = connect(station.address).await;
    client
        .interrogate(Qoi::STATION)
        .await
        .expect("the client runs");
    next_until(
        &mut client,
        answer(
            |b| matches!(b, Body::C_IC_NA_1(_)),
            cause::ACTIVATION_CONFIRMATION,
            false,
        ),
        "the confirmation",
    )
    .await;
    // Point 1 is a single point of group 1; the station interrogation carries it.
    next_until(
        &mut client,
        answer(is_single, cause::INTERROGATED_STATION, false),
        "the single point",
    )
    .await;
    // Point 6 is time-tagged: the interrogation reports it with the plain float type.
    next_until(
        &mut client,
        answer(
            |b| matches!(b, Body::M_ME_NC_1(_)),
            cause::INTERROGATED_STATION,
            false,
        ),
        "the float point",
    )
    .await;
    next_until(
        &mut client,
        answer(
            |b| matches!(b, Body::C_IC_NA_1(_)),
            cause::ACTIVATION_TERMINATION,
            false,
        ),
        "the termination",
    )
    .await;
}

#[tokio::test]
async fn a_command_is_confirmed_at_a_command_point_and_refused_elsewhere() {
    let station = start_station(20);
    let mut client = connect(station.address).await;
    let select = Sco {
        on: true,
        qoc: Qoc::new(0, true).expect("in range"),
    };
    client
        .single_command(address(1), select, None)
        .await
        .expect("the client runs");
    next_until(
        &mut client,
        answer(is_single_command, cause::ACTIVATION_CONFIRMATION, false),
        "the confirmation of the command",
    )
    .await;

    // Point 2 takes no command: the station refuses it as an unknown address (cause 47).
    client
        .single_command(address(2), select, None)
        .await
        .expect("the client runs");
    next_until(
        &mut client,
        answer(is_single_command, cause::UNKNOWN_INFO_OBJECT_ADDRESS, true),
        "the refusal",
    )
    .await;
}

#[tokio::test]
async fn a_simulated_change_reaches_the_controlling_station() {
    // Point 2 is time-tagged and toggles every two seconds: its change is spontaneous (cause 3).
    let station = start_station(20);
    let mut client = connect(station.address).await;
    next_until(
        &mut client,
        answer(is_single_stamped, cause::SPONTANEOUS, false),
        "a change of point 2",
    )
    .await;
}

#[tokio::test]
async fn a_station_that_runs_for_its_duration_stops() {
    let mut station = start_station(1);
    // The process ends after its duration, with success.
    let status = loop {
        if let Some(status) = station.child.try_wait().expect("can be polled") {
            break status;
        }
        sleep(Duration::from_millis(50)).await;
    };
    assert!(status.success(), "the station ends cleanly: {status}");
}

#[tokio::test]
async fn the_station_serves_only_the_points_of_its_toml_file() {
    // The file holds one single-point, so the interrogation answers with that point alone.
    let path = format!("{}/server-points.toml", env!("CARGO_TARGET_TMPDIR"));
    std::fs::write(
        &path,
        "[[point]]\naddress = 9\nkind = \"single\"\ngroup = 1\n",
    )
    .expect("the file is written");
    let station = start_station_with(20, &["--points", &path]);
    let mut client = connect(station.address).await;
    client
        .interrogate(Qoi::STATION)
        .await
        .expect("the client runs");

    let mut types = Vec::new();
    loop {
        let event = next_until(
            &mut client,
            |e| matches!(e, Event::Delivery(Delivery::Asdu(_))),
            "an answer",
        )
        .await;
        let Event::Delivery(Delivery::Asdu(asdu)) = event else {
            continue;
        };
        if asdu.cot.cause() == cause::ACTIVATION_TERMINATION {
            break;
        }
        if asdu.cot.cause() == cause::INTERROGATED_STATION {
            let name = format!("{:?}", asdu.body);
            types.push(name.split('(').next().unwrap_or_default().to_string());
        }
    }
    assert_eq!(types, ["M_SP_NA_1"]);
}
