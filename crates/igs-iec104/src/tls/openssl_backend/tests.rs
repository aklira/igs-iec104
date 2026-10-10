// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! Renegotiation of the OpenSSL backend over loopback (clause 7.4.5): a TLS 1.2 session is
//! renegotiated once its interval has passed, and a TLS 1.3 session never is (clause 8.7).

use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use openssl::pkey::PKey;
use openssl::ssl::{SslConnector, SslMethod, SslOptions};
use openssl::x509::X509;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::{sleep, timeout};

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
    backend_with(certificate, key, interval, tls13, Arc::new(Discard))
}

/// The OpenSSL backend of one station, with its events raised to `events`.
fn backend_with(
    certificate: &[u8],
    key: &[u8],
    interval: Option<Duration>,
    tls13: bool,
    events: Arc<dyn SecurityEvents>,
) -> OpensslBackend {
    let roots = X509::stack_from_pem(fixture!("roots.pem"))
        .expect("roots")
        .iter()
        .map(|root| root.to_der().expect("DER"))
        .collect();
    let trust = TrustAnchors::new(roots).expect("the roots");
    let settings = TlsSettings::new(identity(certificate, key), trust, events).with_tls13(tls13);
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

/// Keeps the security events that a session raises.
#[derive(Default)]
struct Events(Mutex<Vec<SecurityEvent>>);

impl Events {
    fn seen(&self) -> Vec<SecurityEvent> {
        self.0
            .lock()
            .map(|events| events.clone())
            .unwrap_or_default()
    }
}

impl SecurityEvents for Events {
    fn raise(&self, event: SecurityEvent, _detail: &str) {
        if let Ok(mut events) = self.0.lock() {
            events.push(event);
        }
    }
}

/// A client with the test certificate, the given options and the highest version `max`.
async fn peer(tcp: TcpStream, options: SslOptions, max: SslVersion) -> SslStream<TcpStream> {
    let mut builder = SslConnector::builder(SslMethod::tls_client()).expect("a connector");
    let mut store = X509StoreBuilder::new().expect("a store");
    for root in X509::stack_from_pem(fixture!("roots.pem")).expect("roots") {
        store.add_cert(root).expect("a root");
    }
    builder
        .set_verify_cert_store(store.build())
        .expect("the roots");
    let certificate = X509::from_pem(fixture!("client.pem")).expect("the certificate");
    builder
        .set_certificate(&certificate)
        .expect("the certificate");
    let key = PKey::private_key_from_pem(fixture!("client.key")).expect("the key");
    builder.set_private_key(&key).expect("the key");
    builder
        .set_max_proto_version(Some(max))
        .expect("the highest version");
    builder.set_options(options);
    let ssl = builder
        .build()
        .configure()
        .expect("the configuration")
        .into_ssl("localhost")
        .expect("the session");
    let mut stream = SslStream::new(ssl, tcp).expect("the stream");
    Pin::new(&mut stream)
        .connect()
        .await
        .expect("the handshake");
    stream
}

/// A station with the test server certificate, which answers within 200 ms, and a peer with the given
/// options and highest version. The station renegotiates at `renegotiation` and updates its keys at
/// `key_update`. It returns the peer, the station's session and the events of the station.
async fn station_with_peer(
    peer_max: SslVersion,
    options: SslOptions,
    renegotiation: Option<Duration>,
    key_update: Option<Duration>,
) -> (SslStream<TcpStream>, OpensslStream, Arc<Events>) {
    let events = Arc::new(Events::default());
    let shared: Arc<dyn SecurityEvents> = events.clone();
    let station = backend_with(
        fixture!("server.pem"),
        fixture!("server.key"),
        renegotiation,
        true,
        shared,
    )
    .with_key_update(key_update)
    .with_response(Duration::from_millis(200));
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("binds");
    let address = listener.local_addr().expect("an address");
    let (accepted, connected) = tokio::join!(listener.accept(), TcpStream::connect(address));
    let (server_tcp, _) = accepted.expect("accepted");
    let client_tcp = connected.expect("connected");
    let (peer, session) = tokio::join!(
        peer(client_tcp, options, peer_max),
        station.accept_stream(server_tcp)
    );
    (peer, session.expect("the station's handshake"), events)
}

#[tokio::test]
async fn a_peer_that_refuses_renegotiation_ends_the_session_with_the_alarm() {
    let (mut peer, mut session, events) = station_with_peer(
        SslVersion::TLS1_2,
        SslOptions::NO_RENEGOTIATION,
        Some(Duration::from_millis(50)),
        None,
    )
    .await;
    // The peer reads, so that it receives the request and refuses it with an alert.
    tokio::spawn(async move {
        let mut buffer = [0u8; 64];
        while let Ok(read) = peer.read(&mut buffer).await {
            if read == 0 {
                break;
            }
        }
    });

    let mut buffer = [0u8; 16];
    let outcome = timeout(Duration::from_secs(10), session.read(&mut buffer))
        .await
        .expect("the station ends the session within 10 s");
    assert!(outcome.is_err(), "the session ends");
    assert_eq!(
        events.seen(),
        vec![
            SecurityEvent::TlsHandshakeSuccess,
            SecurityEvent::RenegotiationExpired,
        ],
        "the handshake notice, then the alarm of a missed renegotiation"
    );
}

#[tokio::test]
async fn a_peer_that_does_not_answer_a_renegotiation_ends_the_session_with_the_alarm() {
    // The peer neither reads nor answers: the station's timer alone must end the session.
    let (_silent, mut session, events) = station_with_peer(
        SslVersion::TLS1_2,
        SslOptions::empty(),
        Some(Duration::from_millis(50)),
        None,
    )
    .await;

    let mut buffer = [0u8; 16];
    let outcome = timeout(Duration::from_secs(10), session.read(&mut buffer))
        .await
        .expect("the station ends the session within 10 s");
    assert!(outcome.is_err(), "the session ends");
    assert_eq!(
        events.seen(),
        vec![
            SecurityEvent::TlsHandshakeSuccess,
            SecurityEvent::RenegotiationExpired,
        ],
        "the handshake notice, then the alarm of a missed renegotiation"
    );
}

#[tokio::test]
async fn an_idle_tls_12_session_is_renegotiated_by_its_timer_alone() {
    // Nothing is written: the server's read waits, and the interval of its timer wakes it.
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
    // The client reads, so that it answers the renegotiation of the server.
    let reader = tokio::spawn(async move {
        let mut buffer = [0u8; 64];
        while let Ok(read) = client_stream.read(&mut buffer).await {
            if read == 0 {
                break;
            }
        }
    });

    let mut buffer = [0u8; 16];
    let _ = timeout(Duration::from_millis(500), server_stream.read(&mut buffer)).await;
    assert!(
        server_stream.renegotiations() >= 1,
        "the idle session was renegotiated"
    );
    reader.abort();
}

#[tokio::test]
async fn a_tls_13_session_is_updated_and_the_peer_answers() {
    // The client's key update is due at 50 ms. Its request goes out with its next write, and the
    // server answers it when it reads the request.
    let client = backend(fixture!("client.pem"), fixture!("client.key"), None, true)
        .with_key_update(Some(Duration::from_millis(50)));
    let server = backend(fixture!("server.pem"), fixture!("server.key"), None, true);
    let (mut client_stream, mut server_stream) = session(&client, &server).await;
    assert_eq!(
        client_stream.key_updates().written(),
        0,
        "no update before the interval"
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
    assert_eq!(
        client_stream.key_updates().written(),
        1,
        "the client wrote its update"
    );
    assert_eq!(
        server_stream.key_updates().requested_by_peer(),
        1,
        "the server received the request"
    );

    // The server's answer goes out with its next write, and the client reads it.
    let mut answer = [0u8; 4];
    let (written, read) = tokio::join!(
        server_stream.write_all(b"pong"),
        client_stream.read_exact(&mut answer)
    );
    written.expect("the write");
    read.expect("the read");
    assert_eq!(&answer, b"pong");
    assert_eq!(
        client_stream.key_updates().answered(),
        1,
        "the client received the answer"
    );
}

#[tokio::test]
async fn a_peer_that_does_not_answer_a_key_update_ends_the_session_with_the_alarm() {
    // The station's key update is due at 50 ms, and the peer never reads. The station's timer alone
    // writes the KeyUpdate, and ends the session when no answer comes within 200 ms.
    let (_silent, mut session, events) = station_with_peer(
        SslVersion::TLS1_3,
        SslOptions::empty(),
        None,
        Some(Duration::from_millis(50)),
    )
    .await;

    let mut buffer = [0u8; 16];
    let outcome = timeout(Duration::from_secs(10), session.read(&mut buffer))
        .await
        .expect("the station ends the session within 10 s");
    assert!(outcome.is_err(), "the session ends");
    assert_eq!(
        events.seen(),
        vec![
            SecurityEvent::TlsHandshakeSuccess,
            SecurityEvent::KeyUpdateExpired,
        ],
        "the handshake notice, then the alarm of an unanswered key update"
    );
}

#[tokio::test]
async fn a_client_does_not_renegotiate_while_the_station_renegotiates_in_time() {
    // The station asks for a renegotiation every 50 ms. The client's interval is 300 ms, and the
    // station's requests come within each of its intervals: the client asks for none (clause 7.4.5).
    let server = backend(
        fixture!("server.pem"),
        fixture!("server.key"),
        Some(Duration::from_millis(50)),
        false,
    );
    let client = backend(
        fixture!("client.pem"),
        fixture!("client.key"),
        Some(Duration::from_millis(300)),
        false,
    );
    let (mut client_stream, mut server_stream) = session(&client, &server).await;
    let mut client_buffer = [0u8; 16];
    let mut server_buffer = [0u8; 16];
    let (_client_read, _server_read) = tokio::join!(
        timeout(
            Duration::from_millis(700),
            client_stream.read(&mut client_buffer)
        ),
        timeout(
            Duration::from_millis(700),
            server_stream.read(&mut server_buffer)
        ),
    );
    assert!(
        server_stream.renegotiations_requested() >= 1,
        "the station asked for renegotiations"
    );
    assert_eq!(
        client_stream.renegotiations_requested(),
        0,
        "the client asked for none"
    );
}
