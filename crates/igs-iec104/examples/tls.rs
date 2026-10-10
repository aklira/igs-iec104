// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! A controlled station and a controlling station that talk over TLS (IEC 62351-3), in one process.
//! The controlling station interrogates the station, and the example prints every event, including
//! the security events of the handshakes.
//!
//! Run it with five PEM files: the trust anchors, then the certificate and key of the station, then
//! the certificate and key of the controlling station. Both certificates must be valid for the name
//! `localhost`, and each must be signed by one of the trust anchors:
//!
//! ```text
//! cargo run --example tls -- roots.pem station.pem station.key controlling.pem controlling.key
//! ```
//!
//! The test certificates of the crate work: `crates/igs-iec104/tests/fixtures/tls/` holds them, made
//! by `generate.sh` there.

use std::error::Error;
use std::fs;
use std::net::SocketAddr;
use std::sync::Arc;

use igs_iec104::client::{Client, ClientConfig, Event};
use igs_iec104::process_image::PointValue;
use igs_iec104::server::{Handler, Operation, Refusal, Server, ServerConfig};
use igs_iec104::tls::events::{SecurityEvent, SecurityEvents};
use igs_iec104::tls::openssl_backend::OpensslBackend;
use igs_iec104::tls::settings::{Identity, TlsSettings, TrustAnchors};
use igs_iec104::tls::Secure;
use igs_iec104::Delivery;
use igs_iec104_codec::elements::{Qoi, QualityFlags, Siq};
use igs_iec104_codec::header::{cause, CommonAddress, InformationObjectAddress};
use igs_iec104_link::TransferState;
use openssl::pkey::PKey;
use openssl::x509::X509;

/// The common address of the station.
const STATION: u16 = 1;

/// Prints each security event that a handshake raises.
struct Print;

impl SecurityEvents for Print {
    fn raise(&self, event: SecurityEvent, detail: &str) {
        println!(
            "security event {} ({}): {detail}",
            event.mnemonic(),
            event.severity()
        );
    }
}

/// A station that refuses every command: the example only interrogates it.
struct Refuse;

impl Handler for Refuse {
    fn select(&self, _operation: &Operation) -> Result<(), Refusal> {
        Err(Refusal::Rejected)
    }

    fn execute(&self, _operation: &Operation) -> Result<(), Refusal> {
        Err(Refusal::Rejected)
    }
}

/// The DER of every certificate in a PEM file.
fn certificates_der(pem: &[u8]) -> Result<Vec<Vec<u8>>, Box<dyn Error>> {
    let mut der = Vec::new();
    for certificate in X509::stack_from_pem(pem)?.iter() {
        der.push(certificate.to_der()?);
    }
    Ok(der)
}

/// The identity of a station from its PEM files: the certificate chain and the key, in DER.
fn station_identity(
    events: &dyn SecurityEvents,
    certificate: &[u8],
    key: &[u8],
) -> Result<Identity, Box<dyn Error>> {
    let key = PKey::private_key_from_pem(key)?.private_key_to_der()?;
    Ok(Identity::new(certificates_der(certificate)?, key, events)?)
}

/// The TLS backend of one station, from its PEM files.
fn backend(roots: &[u8], certificate: &[u8], key: &[u8]) -> Result<Secure, Box<dyn Error>> {
    let shared: Arc<dyn SecurityEvents> = Arc::new(Print);
    let identity = station_identity(shared.as_ref(), certificate, key)?;
    let trust = TrustAnchors::new(certificates_der(roots)?)?;
    let settings = TlsSettings::new(identity, trust, shared);
    Ok(Secure::new(OpensslBackend::new(&settings)?))
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn Error>> {
    let paths: Vec<String> = std::env::args().skip(1).collect();
    let [roots, station_certificate, station_key, controlling_certificate, controlling_key] =
        paths.as_slice()
    else {
        return Err("give five PEM files: roots, station certificate and key, controlling certificate and key".into());
    };
    let roots = fs::read(roots)?;
    let station_tls = backend(
        &roots,
        &fs::read(station_certificate)?,
        &fs::read(station_key)?,
    )?;
    let controlling_tls = backend(
        &roots,
        &fs::read(controlling_certificate)?,
        &fs::read(controlling_key)?,
    )?;

    let bind: SocketAddr = "127.0.0.1:0".parse()?;
    let config = ServerConfig::new(bind, CommonAddress::new(STATION)).with_tls(station_tls);
    let server = Server::bind(config, Refuse).await?;
    let address = server.local_addr()?;
    let value = PointValue::Single(Siq {
        on: true,
        quality: QualityFlags::default(),
    });
    server.handle().add_point(
        InformationObjectAddress::new(1).ok_or("address 1")?,
        value,
        false,
    )?;
    tokio::spawn(server.run());

    let config = ClientConfig::new(address, CommonAddress::new(STATION))
        .with_tls(controlling_tls, "localhost");
    let mut client = Client::connect(config);
    // The loop ends at the termination of the interrogation. A connection that fails or drops ends
    // the example with its reason, rather than a retry every few seconds.
    while let Some(event) = client.next_event().await {
        println!("{event:?}");
        match event {
            Event::Delivery(Delivery::Transfer(TransferState::Started)) => {
                client.interrogate(Qoi::STATION).await?;
            }
            Event::Connected => client.start_data_transfer().await?,
            Event::ConnectFailed(error) | Event::Disconnected(error) => {
                return Err(format!("the connection failed: {error}").into())
            }
            Event::Delivery(Delivery::Asdu(asdu))
                if asdu.cot.cause() == cause::ACTIVATION_TERMINATION =>
            {
                break;
            }
            _ => {}
        }
    }
    Ok(())
}
