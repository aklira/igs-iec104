// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! Renegotiation of the OpenSSL backend over loopback (clause 7.4.5): a TLS 1.2 session is
//! renegotiated once its interval has passed, and a TLS 1.3 session never is (clause 8.7).

use std::sync::Arc;
use std::time::Duration;

use openssl::pkey::PKey;
use openssl::x509::X509;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::sleep;

use super::*;
use crate::tls::events::Discard;
use crate::tls::settings::{Identity, TlsSettings, TrustAnchors};

macro_rules! fixture {
    ($name:literal) => {
        include_bytes!(concat!("../../../tests/fixtures/tls/", $name)).as_slice()
    };
}

/// The identity of a station, from its PEM certificate and key.
fn identity(certificate: &[u8], key: &[u8]) -> Identity {
    let chain = X509::stack_from_pem(certificate)
        .expect("certificates")
        .iter()
        .map(|certificate| certificate.to_der().expect("DER"))
        .collect();
    let key = PKey::private_key_from_pem(key)
        .expect("a key")
        .private_key_to_der()
        .expect("DER of the key");
    Identity::new(chain, key, &Discard).expect("an identity")
}

/// The OpenSSL backend of one station, with the test roots and the given interval.
fn backend(
    certificate: &[u8],
    key: &[u8],
    interval: Option<Duration>,
    tls13: bool,
) -> OpensslBackend {
    let roots = X509::stack_from_pem(fixture!("roots.pem"))
        .expect("roots")
        .iter()
        .map(|root| root.to_der().expect("DER"))
        .collect();
    let trust = TrustAnchors::new(roots).expect("the roots");
    let settings =
        TlsSettings::new(identity(certificate, key), trust, Arc::new(Discard)).with_tls13(tls13);
    OpensslBackend::new(&settings)
        .expect("the backend")
        .with_renegotiation(interval)
}

/// Connects a client station to a server station over loopback, and returns both streams.
async fn session(
    client: &OpensslBackend,
    server: &OpensslBackend,
) -> (OpensslStream, OpensslStream) {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("binds");
    let address = listener.local_addr().expect("an address");
    let (accepted, connected) = tokio::join!(listener.accept(), TcpStream::connect(address));
    let (server_tcp, _peer) = accepted.expect("accepted");
    let client_tcp = connected.expect("connected");
    let (client_stream, server_stream) = tokio::join!(
        client.connect_stream(client_tcp, "localhost"),
        server.accept_stream(server_tcp)
    );
    (
        client_stream.expect("the client handshake"),
        server_stream.expect("the server handshake"),
    )
}

#[tokio::test]
async fn a_tls_12_session_is_renegotiated_once_its_interval_has_passed() {
    let interval = Some(Duration::from_millis(50));
    let client = backend(
        fixture!("client.pem"),
        fixture!("client.key"),
        interval,
        false,
    );
    let server = backend(
        fixture!("server.pem"),
        fixture!("server.key"),
        interval,
        false,
    );
    let (mut client_stream, mut server_stream) = session(&client, &server).await;
    assert_eq!(
        client_stream.renegotiations(),
        0,
        "no renegotiation before the interval"
    );

    sleep(Duration::from_millis(100)).await;
    let mut received = [0u8; 4];
    let (written, read) = tokio::join!(
        client_stream.write_all(b"ping"),
        server_stream.read_exact(&mut received)
    );
    written.expect("the write");
    read.expect("the read");
    assert_eq!(&received, b"ping");
    assert!(
        client_stream.renegotiations() >= 1,
        "the client renegotiated"
    );
    assert!(
        server_stream.renegotiations() >= 1,
        "the server renegotiated"
    );

    // The renegotiated session still carries data in both directions.
    let (written, read) = tokio::join!(
        server_stream.write_all(b"pong"),
        client_stream.read_exact(&mut received)
    );
    written.expect("the write after the renegotiation");
    read.expect("the read after the renegotiation");
    assert_eq!(&received, b"pong");
}

#[tokio::test]
async fn a_tls_13_session_is_never_renegotiated() {
    let interval = Some(Duration::from_millis(50));
    let client = backend(
        fixture!("client.pem"),
        fixture!("client.key"),
        interval,
        true,
    );
    let server = backend(
        fixture!("server.pem"),
        fixture!("server.key"),
        interval,
        true,
    );
    let (mut client_stream, mut server_stream) = session(&client, &server).await;

    sleep(Duration::from_millis(100)).await;
    let mut received = [0u8; 4];
    let (written, read) = tokio::join!(
        client_stream.write_all(b"ping"),
        server_stream.read_exact(&mut received)
    );
    written.expect("the write");
    read.expect("the read");
    assert_eq!(&received, b"ping");
    assert_eq!(client_stream.renegotiations(), 0);
    assert_eq!(server_stream.renegotiations(), 0);
}
