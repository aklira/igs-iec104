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

use std::io::{BufRead, BufReader};
use std::net::SocketAddr;
use std::process::{Child, ChildStdout, Command as Process, Stdio};
use std::time::Duration;

use igs_iec104::client::{Client, ClientConfig, Event};
use igs_iec104::Delivery;
use igs_iec104_codec::asdu::Body;
use igs_iec104_codec::elements::{Qoc, Qoi, Sco};
use igs_iec104_codec::header::{cause, CommonAddress, InformationObjectAddress};
use igs_iec104_link::TransferState;
use tokio::time::{sleep, timeout};

const STATION: u16 = 1;

/// The demo station as a child process, stopped when the guard is dropped. Its standard output stays
/// open, so that the station can print after the line that gives its address.
struct Station {
    child: Child,
    address: SocketAddr,
    _output: BufReader<ChildStdout>,
}

impl Drop for Station {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Starts the station with `extra` options, after its common address and its duration. The station
/// binds port 0 and reports the address it got, so two stations never race for one free port.
fn start_station_with(seconds: u64, extra: &[&str]) -> Station {
    let mut child = Process::new(env!("CARGO_BIN_EXE_igs104-server"))
        .args([
            "--bind",
            "127.0.0.1:0",
            "-c",
            &STATION.to_string(),
            "--for",
            &seconds.to_string(),
        ])
        .args(extra)
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("the server starts");
    let output = child.stdout.take().expect("the standard output is piped");
    let mut output = BufReader::new(output);
    let mut line = String::new();
    output
        .read_line(&mut line)
        .expect("the station reports its address");
    // The line is "igs104-server: station N on ADDRESS".
    let address: SocketAddr = line
        .split_whitespace()
        .last()
        .and_then(|text| text.parse().ok())
        .expect("the station reports its address");
    Station {
        child,
        address,
        _output: output,
    }
}

fn start_station(seconds: u64) -> Station {
    start_station_with(seconds, &[])
}

/// Connects to the station, once it listens, and starts the data transfer.
async fn connect(address: SocketAddr) -> Client {
    connect_with(ClientConfig::new(address, CommonAddress::new(STATION))).await
}

/// Like [`connect`], with the configuration given (a TLS client, for example).
async fn connect_with(config: ClientConfig) -> Client {
    let mut client = Client::connect(config);
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

/// A TLS client of the station: its certificate, and the roots of the fixtures (the test certificates of the
/// crate, `tests/fixtures/tls`).
#[cfg(feature = "openssl")]
fn tls_client() -> igs_iec104::tls::Secure {
    use std::sync::Arc;

    use igs_iec104::tls::events::{SecurityEvent, SecurityEvents};
    use igs_iec104::tls::openssl_backend::OpensslBackend;
    use igs_iec104::tls::settings::{Identity, TlsSettings, TrustAnchors};
    use igs_iec104::tls::Secure;
    use openssl::pkey::PKey;
    use openssl::x509::X509;

    struct Quiet;
    impl SecurityEvents for Quiet {
        fn raise(&self, _event: SecurityEvent, _detail: &str) {}
    }
    let fixtures = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/tls");
    let read = |name: &str| std::fs::read(format!("{fixtures}/{name}")).expect("a fixture");
    let der = |pem: &[u8]| -> Vec<Vec<u8>> {
        X509::stack_from_pem(pem)
            .expect("certificates")
            .iter()
            .map(|certificate| certificate.to_der().expect("DER"))
            .collect()
    };
    let chain = der(&read("client.pem"));
    let key = PKey::private_key_from_pem(&read("client.key"))
        .expect("a key")
        .private_key_to_der()
        .expect("DER of the key");
    let identity = Identity::new(chain, key, &Quiet).expect("the identity");
    let trust = TrustAnchors::new(der(&read("roots.pem"))).expect("the roots");
    let settings = TlsSettings::new(identity, trust, Arc::new(Quiet));
    Secure::new(OpensslBackend::new(&settings).expect("the backend"))
}

#[cfg(feature = "openssl")]
#[tokio::test]
async fn the_station_serves_tls_to_a_client_that_presents_a_certificate() {
    // The station with its identity, the roots of the client certificates (IEC 62351-3): the client
    // starts the data transfer over TLS.
    let fixtures = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/tls");
    let station = start_station_with(
        60,
        &[
            "--tls-roots",
            &format!("{fixtures}/roots.pem"),
            "--tls-certificate",
            &format!("{fixtures}/server.pem"),
            "--tls-key",
            &format!("{fixtures}/server.key"),
        ],
    );
    let config = ClientConfig::new(station.address, CommonAddress::new(STATION))
        .with_tls(tls_client(), "localhost");
    let _client = connect_with(config).await;
}

#[cfg(feature = "openssl")]
#[tokio::test]
async fn a_plain_client_gets_no_transfer_from_a_tls_station() {
    let fixtures = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/tls");
    let station = start_station_with(
        60,
        &[
            "--tls-roots",
            &format!("{fixtures}/roots.pem"),
            "--tls-certificate",
            &format!("{fixtures}/server.pem"),
            "--tls-key",
            &format!("{fixtures}/server.key"),
        ],
    );
    let mut client = Client::connect(ClientConfig::new(
        station.address,
        CommonAddress::new(STATION),
    ));
    client.start_data_transfer().await.expect("the client runs");
    // The station does not speak plain IEC 60870-5-104 on a TLS port: no transfer starts.
    let started = timeout(Duration::from_secs(5), async {
        loop {
            match client.next_event().await {
                Some(Event::Delivery(Delivery::Transfer(TransferState::Started))) => return true,
                Some(_) => continue,
                None => return false,
            }
        }
    })
    .await;
    assert!(
        !matches!(started, Ok(true)),
        "a plain client started a transfer"
    );
}

#[test]
fn the_tls_options_come_together() {
    let station = Process::new(env!("CARGO_BIN_EXE_igs104-server"))
        .args([
            "--bind",
            "127.0.0.1:0",
            "--tls-roots",
            "/nonexistent/roots.pem",
        ])
        .output()
        .expect("the server runs");
    assert!(!station.status.success());
    let message = String::from_utf8_lossy(&station.stderr);
    assert!(message.contains("need --tls-roots"), "{message}");
}
