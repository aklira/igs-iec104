// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! The command-line client `igs104-client` over TLS (task X1), against the demo station `igs104-server`
//! with its TLS options: the client presents its certificate, and checks the station's certificate against
//! the trust anchors it is given.
#![cfg(feature = "openssl")]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use std::io::{BufRead, BufReader};
use std::net::SocketAddr;
use std::process::{Child, ChildStdout, Command, Stdio};

const STATION: u16 = 1;

/// The fixtures of the TLS tests of the crate, as paths.
fn fixture(name: &str) -> String {
    format!("{}/tests/fixtures/tls/{name}", env!("CARGO_MANIFEST_DIR"))
}

/// A TLS station as a child process, stopped when dropped. It binds port 0 and reports its address.
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

fn start_tls_station() -> Station {
    let mut child = Command::new(env!("CARGO_BIN_EXE_igs104-server"))
        .args([
            "--bind",
            "127.0.0.1:0",
            "-c",
            &STATION.to_string(),
            "--for",
            "60",
            "--tls-roots",
            &fixture("roots.pem"),
            "--tls-certificate",
            &fixture("server.pem"),
            "--tls-key",
            &fixture("server.key"),
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("the station starts");
    let mut output = BufReader::new(child.stdout.take().expect("the output is piped"));
    let mut line = String::new();
    output
        .read_line(&mut line)
        .expect("the station reports its address");
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

/// The client, with the TLS options given, runs an interrogation of the station.
fn interrogate(
    station: &Station,
    roots: &str,
    certificate: &str,
    key: &str,
) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_igs104-client"))
        .args([
            "--address",
            &station.address.to_string(),
            "-c",
            &STATION.to_string(),
            "--timeout",
            "10",
            "--tls-roots",
            roots,
            "--tls-certificate",
            certificate,
            "--tls-key",
            key,
            "interrogate",
        ])
        .output()
        .expect("the client runs")
}

#[test]
fn the_client_interrogates_a_tls_station_with_its_certificate() {
    let station = start_tls_station();
    let output = interrogate(
        &station,
        &fixture("roots.pem"),
        &fixture("client.pem"),
        &fixture("client.key"),
    );
    let text = String::from_utf8_lossy(&output.stdout);
    let errors = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "the interrogation failed:\n{text}{errors}"
    );
    assert!(
        text.contains("security event TLS_HS_SUCCESS"),
        "the handshake notice:\n{text}"
    );
}

#[test]
fn the_client_refuses_a_station_it_does_not_trust() {
    // The client trusts its own certificate as the only anchor: the station's certificate is not under it.
    let station = start_tls_station();
    let output = interrogate(
        &station,
        &fixture("client.pem"),
        &fixture("client.pem"),
        &fixture("client.key"),
    );
    assert!(
        !output.status.success(),
        "the client accepted an untrusted station"
    );
}
