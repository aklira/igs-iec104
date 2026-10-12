// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! The TLS options of `igs104-server` (task X1): the trust anchors, and the identity of the station from
//! PEM files. The client certificate is required (IEC 62351-3). Each security event is printed as it is
//! raised, as the TLS example does, so that a run keeps the evidence of its handshakes.

use std::path::Path;
use std::sync::Arc;

use igs_iec104::tls::events::{SecurityEvent, SecurityEvents};
use igs_iec104::tls::openssl_backend::OpensslBackend;
use igs_iec104::tls::settings::{Identity, TlsSettings, TrustAnchors};
use igs_iec104::tls::Secure;
use openssl::pkey::PKey;
use openssl::x509::X509;

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

/// The TLS backend of the station, from its three PEM files: the trust anchors, the certificate chain and
/// the key of the station.
pub fn backend(roots: &Path, certificate: &Path, key: &Path) -> Result<Secure, String> {
    let read =
        |path: &Path| std::fs::read(path).map_err(|error| format!("{}: {error}", path.display()));
    let shared: Arc<dyn SecurityEvents> = Arc::new(Print);
    let certificate_pem = read(certificate)?;
    let key_pem = read(key)?;
    let chain = X509::stack_from_pem(&certificate_pem)
        .map_err(|error| format!("{}: {error}", certificate.display()))?
        .iter()
        .map(|certificate| certificate.to_der().map_err(|error| error.to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    let key = PKey::private_key_from_pem(&key_pem)
        .and_then(|key| key.private_key_to_der())
        .map_err(|error| format!("{}: {error}", key.display()))?;
    let identity = Identity::new(chain, key, shared.as_ref()).map_err(|error| error.to_string())?;
    let roots_pem = read(roots)?;
    let anchors = X509::stack_from_pem(&roots_pem)
        .map_err(|error| format!("{}: {error}", roots.display()))?
        .iter()
        .map(|certificate| certificate.to_der().map_err(|error| error.to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    let trust = TrustAnchors::new(anchors).map_err(|error| error.to_string())?;
    let settings = TlsSettings::new(identity, trust, shared);
    let backend = OpensslBackend::new(&settings).map_err(|error| error.to_string())?;
    Ok(Secure::new(backend))
}
