// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! The rustls backend of the TLS profile (task X1). It offers TLS 1.2 and TLS 1.3 with the cipher
//! suites and groups of the profile that rustls implements, and it requires a certificate from
//! both peers (clause 6.4.3).
//!
//! rustls has no TLS 1.0 or 1.1, no renegotiation, no static RSA key exchange, no finite-field
//! DHE, no CCM and no NULL encryption. The conformance gaps that follow are recorded in
//! `docs/ai-log/X1.md`. The sans-I/O connections are the base of the tokio stream. The renegotiation interval of
//! the settings is not applied by this backend: its sessions are not renegotiated.

use std::fmt;
use std::io;
use std::sync::Arc;

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::client::WebPkiServerVerifier;
use rustls::crypto::CryptoProvider;
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{
    CertificateDer, CertificateRevocationListDer, PrivateKeyDer, ServerName, UnixTime,
};
use rustls::server::danger::{ClientCertVerified, ClientCertVerifier};
use rustls::server::WebPkiClientVerifier;
use rustls::version::{TLS12, TLS13};
use rustls::{
    CertificateError, ClientConfig, ClientConnection, DigitallySignedStruct, DistinguishedName,
    Error as RustlsError, RootCertStore, ServerConfig, ServerConnection, SignatureScheme,
    SupportedProtocolVersion,
};

use tokio::net::TcpStream;
use tokio_rustls::{TlsAcceptor, TlsConnector};

use super::events::{SecurityEvent, SecurityEvents};
use super::profile;
use super::settings::{Identity, TlsSettings, TrustAnchors};
use super::{Handshake, TlsBackend, TlsError};
use crate::transport::BoxedTransport;

/// The client and server configurations of one station, built from its settings.
pub struct RustlsBackend {
    client: Arc<ClientConfig>,
    server: Arc<ServerConfig>,
    events: Arc<dyn SecurityEvents>,
}

impl RustlsBackend {
    /// Builds the configurations. Fails when the settings break the profile, or when the backend
    /// cannot provide them (for example TLS 1.0 or the NULL suites).
    pub fn new(settings: &TlsSettings) -> Result<Self, TlsError> {
        settings.validate()?;
        let events = Arc::clone(settings.events());
        let provider = Arc::new(profile_provider());
        let versions = protocol_versions(settings.tls13());
        let roots = Arc::new(root_store(settings.trust())?);
        let crls = revocation_lists(settings.revocation());
        let limit = settings.max_certificate_size();

        let client_verifier =
            WebPkiClientVerifier::builder_with_provider(Arc::clone(&roots), Arc::clone(&provider))
                .with_crls(crls.clone())
                .build()
                .map_err(|error| TlsError::Invalid(format!("client certificate check: {error}")))?;
        let client_verifier = Arc::new(LimitedClientVerifier {
            inner: client_verifier,
            limit: SizeLimit::new(limit, Arc::clone(&events)),
        });
        let server = ServerConfig::builder_with_provider(Arc::clone(&provider))
            .with_protocol_versions(&versions)
            .map_err(|error| TlsError::Invalid(error.to_string()))?
            .with_client_cert_verifier(client_verifier)
            .with_single_cert(certificates(settings.identity()), key(settings.identity())?)
            .map_err(|error| identity_error(events.as_ref(), &error))?;

        let server_verifier = WebPkiServerVerifier::builder_with_provider(roots, provider.clone())
            .with_crls(crls)
            .build()
            .map_err(|error| TlsError::Invalid(format!("server certificate check: {error}")))?;
        let server_verifier = Arc::new(LimitedServerVerifier {
            inner: server_verifier,
            limit: SizeLimit::new(limit, Arc::clone(&events)),
        });
        // The custom verifier is the WebPKI verifier of rustls, with the revocation lists and the
        // size limit added in front. rustls names every custom verifier "dangerous"; the checks
        // that the profile asks for are all made by the verifier above.
        let client = ClientConfig::builder_with_provider(provider)
            .with_protocol_versions(&versions)
            .map_err(|error| TlsError::Invalid(error.to_string()))?
            .dangerous()
            .with_custom_certificate_verifier(server_verifier)
            .with_client_auth_cert(certificates(settings.identity()), key(settings.identity())?)
            .map_err(|error| identity_error(events.as_ref(), &error))?;

        Ok(Self {
            client: Arc::new(client),
            server: Arc::new(server),
            events,
        })
    }

    /// A sans-I/O client connection to the station `server_name`.
    pub fn client_connection(
        &self,
        server_name: ServerName<'static>,
    ) -> Result<ClientConnection, TlsError> {
        ClientConnection::new(Arc::clone(&self.client), server_name)
            .map_err(|error| TlsError::Invalid(error.to_string()))
    }

    /// A sans-I/O server connection for an accepted TCP connection.
    pub fn server_connection(&self) -> Result<ServerConnection, TlsError> {
        ServerConnection::new(Arc::clone(&self.server))
            .map_err(|error| TlsError::Invalid(error.to_string()))
    }

    /// The security events of this backend.
    pub fn events(&self) -> &Arc<dyn SecurityEvents> {
        &self.events
    }
}

impl TlsBackend for RustlsBackend {
    fn connect<'a>(&'a self, tcp: TcpStream, host: &'a str) -> Handshake<'a> {
        Box::pin(async move {
            let name = ServerName::try_from(host)
                .map_err(|error| TlsError::Invalid(format!("the host {host}: {error}")))?
                .to_owned();
            let connector = TlsConnector::from(Arc::clone(&self.client));
            let stream = connector
                .connect(name, tcp)
                .await
                .map_err(|error| self.handshake_error(&error))?;
            raise_handshake_success(self.events.as_ref());
            Ok(Box::new(stream) as BoxedTransport)
        })
    }

    fn accept<'a>(&'a self, tcp: TcpStream) -> Handshake<'a> {
        Box::pin(async move {
            let acceptor = TlsAcceptor::from(Arc::clone(&self.server));
            let stream = acceptor
                .accept(tcp)
                .await
                .map_err(|error| self.handshake_error(&error))?;
            raise_handshake_success(self.events.as_ref());
            Ok(Box::new(stream) as BoxedTransport)
        })
    }
}

impl RustlsBackend {
    /// Maps an error of the tokio handshake to the events of the profile. A failure of the
    /// TCP stream itself raises nothing; it is a plain transport error.
    fn handshake_error(&self, error: &io::Error) -> TlsError {
        if let Some(rustls_error) = error
            .get_ref()
            .and_then(|inner| inner.downcast_ref::<RustlsError>())
        {
            raise_handshake_failure(self.events.as_ref(), rustls_error);
        }
        TlsError::Handshake(error.to_string())
    }
}

/// Reads an identity from PEM: the certificates, the leaf first, and the private key. A reading
/// or matching failure raises [`SecurityEvent::NoLocalCertificate`].
pub fn identity_from_pem(
    events: &dyn SecurityEvents,
    certificates: &[u8],
    key: &[u8],
) -> Result<Identity, TlsError> {
    let chain: Result<Vec<Vec<u8>>, _> = CertificateDer::pem_slice_iter(certificates)
        .map(|der| der.map(|der| der.to_vec()))
        .collect();
    let key = PrivateKeyDer::from_pem_slice(key).map(|key| key.secret_der().to_vec());
    match (chain, key) {
        (Ok(chain), Ok(key)) => Identity::new(chain, key, events),
        _ => {
            events.raise(SecurityEvent::NoLocalCertificate, "the PEM does not parse");
            Err(TlsError::Identity(
                "the certificate or the key does not parse".into(),
            ))
        }
    }
}

/// Reads trust anchors from a PEM bundle.
pub fn trust_anchors_from_pem(pem: &[u8]) -> Result<TrustAnchors, TlsError> {
    let roots: Result<Vec<Vec<u8>>, _> = CertificateDer::pem_slice_iter(pem)
        .map(|der| der.map(|der| der.to_vec()))
        .collect();
    let roots = roots.map_err(|error| TlsError::Identity(format!("trust anchors: {error}")))?;
    TrustAnchors::new(roots)
}

/// Reads revocation lists from a PEM bundle, in DER.
pub fn revocation_from_pem(pem: &[u8]) -> Result<Vec<Vec<u8>>, TlsError> {
    let crls: Result<Vec<Vec<u8>>, _> = CertificateRevocationListDer::pem_slice_iter(pem)
        .map(|der| der.map(|der| der.to_vec()))
        .collect();
    crls.map_err(|error| TlsError::Identity(format!("revocation lists: {error}")))
}

/// Raises the security events that a failed handshake stands for (Annex A). An error without an
/// event of the profile raises nothing.
pub fn raise_handshake_failure(events: &dyn SecurityEvents, error: &RustlsError) {
    for event in failure_events(error) {
        events.raise(*event, &error.to_string());
    }
}

/// Raises the notice that a handshake succeeded (clause 6.1).
pub fn raise_handshake_success(events: &dyn SecurityEvents) {
    events.raise(SecurityEvent::TlsHandshakeSuccess, "");
}

/// The events of a handshake error. A revoked certificate also terminates the session (clause
/// 6.4.4.4.1), so both events are raised; Q-024 asks whether that holds for a refused handshake.
fn failure_events(error: &RustlsError) -> &'static [SecurityEvent] {
    match error {
        RustlsError::NoCertificatesPresented => &[SecurityEvent::NoPeerCertificate],
        RustlsError::InvalidCertificate(error) => match error {
            CertificateError::Expired | CertificateError::ExpiredContext { .. } => {
                &[SecurityEvent::CertificateExpired]
            }
            CertificateError::UnknownIssuer => &[SecurityEvent::NoCaMatch],
            CertificateError::Revoked => &[
                SecurityEvent::CertificateRevoked,
                SecurityEvent::RevokedSessionClosed,
            ],
            CertificateError::BadSignature => &[SecurityEvent::SignatureNotVerified],
            CertificateError::UnsupportedSignatureAlgorithmContext { .. }
            | CertificateError::UnsupportedSignatureAlgorithmForPublicKeyContext { .. } => {
                &[SecurityEvent::SignatureAlgorithmNotSupported]
            }
            _ => &[],
        },
        _ => &[],
    }
}

fn identity_error(events: &dyn SecurityEvents, error: &RustlsError) -> TlsError {
    events.raise(SecurityEvent::NoLocalCertificate, &error.to_string());
    TlsError::Identity(error.to_string())
}

/// The suites and groups of the profile, from the provider of the ring crate.
fn profile_provider() -> CryptoProvider {
    let mut provider = rustls::crypto::ring::default_provider();
    provider
        .cipher_suites
        .retain(|suite| profile::allows(u16::from(suite.suite())));
    provider
        .kx_groups
        .retain(|group| profile::allows_group(u16::from(group.name())));
    provider
}

fn protocol_versions(tls13: bool) -> Vec<&'static SupportedProtocolVersion> {
    if tls13 {
        vec![&TLS12, &TLS13]
    } else {
        vec![&TLS12]
    }
}

fn root_store(trust: &TrustAnchors) -> Result<RootCertStore, TlsError> {
    let mut roots = RootCertStore::empty();
    for der in trust.roots() {
        roots
            .add(CertificateDer::from(der.clone()))
            .map_err(|error| TlsError::Identity(format!("trust anchor: {error}")))?;
    }
    Ok(roots)
}

fn revocation_lists(crls: &[Vec<u8>]) -> Vec<CertificateRevocationListDer<'static>> {
    crls.iter()
        .map(|der| CertificateRevocationListDer::from(der.clone()))
        .collect()
}

fn certificates(identity: &Identity) -> Vec<CertificateDer<'static>> {
    identity
        .chain()
        .iter()
        .map(|der| CertificateDer::from(der.clone()))
        .collect()
}

fn key(identity: &Identity) -> Result<PrivateKeyDer<'static>, TlsError> {
    PrivateKeyDer::try_from(identity.key().to_vec())
        .map_err(|error| TlsError::Identity(format!("private key: {error}")))
}

/// The size check of clause 6.4.2. A certificate above the limit is refused, and the event is
/// raised by the side that checks it.
#[derive(Clone)]
struct SizeLimit {
    limit: usize,
    events: Arc<dyn SecurityEvents>,
}

impl fmt::Debug for SizeLimit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SizeLimit")
            .field("limit", &self.limit)
            .finish()
    }
}

impl SizeLimit {
    fn new(limit: usize, events: Arc<dyn SecurityEvents>) -> Self {
        Self { limit, events }
    }

    fn check(&self, end_entity: &CertificateDer<'_>) -> Result<(), RustlsError> {
        let size = end_entity.len();
        if size > self.limit {
            self.events.raise(
                SecurityEvent::CertificateSizeExceeded,
                &format!("{size} octets"),
            );
            return Err(RustlsError::InvalidCertificate(
                CertificateError::ApplicationVerificationFailure,
            ));
        }
        Ok(())
    }
}

/// The WebPKI verifier of the server certificate, behind the size check.
#[derive(Debug)]
struct LimitedServerVerifier {
    inner: Arc<WebPkiServerVerifier>,
    limit: SizeLimit,
}

impl ServerCertVerifier for LimitedServerVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        server_name: &ServerName<'_>,
        ocsp_response: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, RustlsError> {
        self.limit.check(end_entity)?;
        self.inner
            .verify_server_cert(end_entity, intermediates, server_name, ocsp_response, now)
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, RustlsError> {
        self.inner.verify_tls12_signature(message, cert, dss)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, RustlsError> {
        self.inner.verify_tls13_signature(message, cert, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.inner.supported_verify_schemes()
    }
}

/// The WebPKI verifier of the client certificate, behind the size check.
#[derive(Debug)]
struct LimitedClientVerifier {
    inner: Arc<dyn ClientCertVerifier>,
    limit: SizeLimit,
}

impl ClientCertVerifier for LimitedClientVerifier {
    fn root_hint_subjects(&self) -> &[DistinguishedName] {
        self.inner.root_hint_subjects()
    }

    fn verify_client_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        now: UnixTime,
    ) -> Result<ClientCertVerified, RustlsError> {
        self.limit.check(end_entity)?;
        self.inner
            .verify_client_cert(end_entity, intermediates, now)
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, RustlsError> {
        self.inner.verify_tls12_signature(message, cert, dss)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, RustlsError> {
        self.inner.verify_tls13_signature(message, cert, dss)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.inner.supported_verify_schemes()
    }
}

#[cfg(test)]
mod tests;
