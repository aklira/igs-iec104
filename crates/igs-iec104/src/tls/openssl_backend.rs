// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! The OpenSSL backend of the TLS profile (task X1, feature `openssl`). It offers TLS 1.2 and TLS
//! 1.3 with the cipher suites, groups and signature algorithms of the profile, including the ones
//! that rustls cannot provide: the CBC and DHE suites of TLS 1.2, the CCM suites of TLS 1.3, the
//! ffdhe groups and RSA-PSS with the pss key type.
//!
//! Every certificate is judged in the verify callback: its size (clause 6.4.2), its validity, the
//! trust anchor it chains to, and its revocation in the CRLs (clause 6.4.4.4). The callback raises
//! the security events of Annex A for the checks it makes.
//!
//! TLS 1.2 sessions are renegotiated once the interval of the settings has passed (clause 7.4.5). The
//! `openssl` crate has no wrapper for renegotiation: the call goes through `openssl_renegotiation`, the
//! only module of igs-iec104 that contains unsafe code (Q-028, approved).
//!
//! This is the default backend. Each mandatory item of the profile has a test that negotiates it
//! (`every_mandatory_item_of_the_profile_is_negotiated`). Not done yet: the key update interval of
//! clause 8.4, and the check of clause 7.4.5 that the peer renegotiates in time (docs/ai-log/X1.md).

use std::io;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use openssl::asn1::Asn1Time;
use openssl::pkey::PKey;
use openssl::ssl::{
    Ssl, SslConnector, SslContext, SslContextBuilder, SslMethod, SslVerifyMode, SslVersion,
};
use openssl::x509::store::{X509Store, X509StoreBuilder};
use openssl::x509::{CrlStatus, X509Crl, X509StoreContextRef, X509};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpStream;
use tokio_openssl::SslStream;

use super::openssl_renegotiation;

use super::events::{SecurityEvent, SecurityEvents};
use super::settings::TlsSettings;
use super::{Handshake, TlsBackend, TlsError};
use crate::transport::BoxedTransport;

/// The OpenSSL names of the TLS 1.2 cipher suites of the profile (table 9 of clause 10.5.1).
const TLS12_CIPHERS: &str = "ECDHE-ECDSA-AES128-GCM-SHA256:ECDHE-ECDSA-AES256-GCM-SHA384:\
    ECDHE-RSA-AES128-GCM-SHA256:ECDHE-RSA-AES256-GCM-SHA384:DHE-RSA-AES128-GCM-SHA256:\
    DHE-RSA-AES256-GCM-SHA384:AES128-SHA256";

/// The names of the TLS 1.3 cipher suites of the profile (table 13 of clause 10.6.1).
const TLS13_CIPHERSUITES: &str = "TLS_AES_128_GCM_SHA256:TLS_AES_256_GCM_SHA384:\
    TLS_CHACHA20_POLY1305_SHA256:TLS_AES_128_CCM_SHA256:TLS_AES_128_CCM_8_SHA256";

/// The groups of the profile (table 4 of clause 8.3.3, table 16 of clause 10.6.2).
const GROUPS: &str = "P-256:P-384:ffdhe2048:ffdhe3072:ffdhe4096:brainpoolP256r1:brainpoolP384r1:\
    brainpoolP512r1";

/// The signature algorithms of the profile (tables 5 and 6 of clause 8.3.4; tables 17 and 18 of
/// clause 10.6.2). The TLS 1.2 names RSA+SHA256 and ECDSA+SHA256 are the same schemes as
/// rsa_pkcs1_sha256 and ecdsa_secp256r1_sha256, which TLS 1.2 uses too. OpenSSL 3.0 rejects a list
/// that names one scheme twice (OpenSSL 3.5 accepts it), so the TLS 1.2 names are not listed.
const SIGALGS: &str = "rsa_pss_rsae_sha256:rsa_pss_pss_sha256:ecdsa_secp256r1_sha256:\
    rsa_pkcs1_sha256";

/// The message of the OpenSSL reason for a client that sent no certificate when one was required.
const NO_CLIENT_CERTIFICATE: &str = "peer did not return a certificate";

/// The OpenSSL backend: the client and server contexts of one station.
pub struct OpensslBackend {
    connector: SslConnector,
    server: SslContext,
    events: Arc<dyn SecurityEvents>,
    renegotiation: Option<Duration>,
}

impl OpensslBackend {
    /// Builds the contexts from the settings. Fails when the settings break the profile, or a
    /// certificate, key or revocation list does not parse.
    pub fn new(settings: &TlsSettings) -> Result<Self, TlsError> {
        settings.validate()?;
        let events = Arc::clone(settings.events());
        let roots: Vec<Vec<u8>> = settings.trust().roots().to_vec();

        let mut connector = SslConnector::builder(SslMethod::tls_client())
            .map_err(|error| TlsError::Invalid(error.to_string()))?;
        configure(&mut connector, settings, &roots)?;
        let client_judge = judge(settings, &events)?;
        connector.set_verify_callback(SslVerifyMode::PEER, move |ok, ctx| {
            client_judge.check(ok, ctx)
        });

        let mut server = SslContextBuilder::new(SslMethod::tls_server())
            .map_err(|error| TlsError::Invalid(error.to_string()))?;
        configure(&mut server, settings, &roots)?;
        let server_judge = judge(settings, &events)?;
        server.set_verify_callback(
            SslVerifyMode::PEER | SslVerifyMode::FAIL_IF_NO_PEER_CERT,
            move |ok, ctx| server_judge.check(ok, ctx),
        );

        Ok(Self {
            connector: connector.build(),
            server: server.build(),
            events,
            renegotiation: settings.renegotiation(),
        })
    }
}

/// The judge of the certificates of one context: the settings it checks against.
fn judge(settings: &TlsSettings, events: &Arc<dyn SecurityEvents>) -> Result<Judge, TlsError> {
    Ok(Judge {
        roots: settings.trust().roots().to_vec(),
        crls: parse_crls(settings.revocation())?,
        limit: settings.max_certificate_size(),
        events: Arc::clone(events),
    })
}

/// Sets the versions, the suites, the groups, the signature algorithms, the identity and the trust
/// store of one context.
fn configure(
    context: &mut SslContextBuilder,
    settings: &TlsSettings,
    roots: &[Vec<u8>],
) -> Result<(), TlsError> {
    let invalid = |error: openssl::error::ErrorStack| TlsError::Invalid(error.to_string());
    context
        .set_min_proto_version(Some(SslVersion::TLS1_2))
        .map_err(invalid)?;
    let max = if settings.tls13() {
        SslVersion::TLS1_3
    } else {
        SslVersion::TLS1_2
    };
    context.set_max_proto_version(Some(max)).map_err(invalid)?;
    context.set_cipher_list(TLS12_CIPHERS).map_err(invalid)?;
    context
        .set_ciphersuites(TLS13_CIPHERSUITES)
        .map_err(invalid)?;
    context.set_groups_list(GROUPS).map_err(invalid)?;
    context.set_sigalgs_list(SIGALGS).map_err(invalid)?;
    context.set_security_level(2);
    // The DHE suites of TLS 1.2 need Diffie-Hellman parameters in the station. The crate offers the
    // RFC 5114 groups; the ffdhe groups of TLS 1.3 come from the group list above.
    let dh =
        openssl::dh::Dh::get_2048_256().map_err(|error| TlsError::Invalid(error.to_string()))?;
    context
        .set_tmp_dh(&dh)
        .map_err(|error| TlsError::Invalid(error.to_string()))?;

    let identity = settings.identity();
    let mut chain = identity.chain().iter();
    let leaf = chain
        .next()
        .ok_or_else(|| TlsError::Identity("no certificate".into()))?;
    let leaf = parse_certificate(leaf)?;
    context
        .set_certificate(&leaf)
        .map_err(|error| TlsError::Identity(error.to_string()))?;
    for intermediate in chain {
        context
            .add_extra_chain_cert(parse_certificate(intermediate)?)
            .map_err(|error| TlsError::Identity(error.to_string()))?;
    }
    let key = PKey::private_key_from_der(identity.key())
        .map_err(|error| TlsError::Identity(format!("private key: {error}")))?;
    context
        .set_private_key(&key)
        .map_err(|error| TlsError::Identity(format!("private key: {error}")))?;
    context
        .set_verify_cert_store(trust_store(roots)?)
        .map_err(|error| TlsError::Identity(error.to_string()))?;
    Ok(())
}

fn parse_certificate(der: &[u8]) -> Result<X509, TlsError> {
    X509::from_der(der).map_err(|error| TlsError::Identity(format!("certificate: {error}")))
}

fn parse_crls(crls: &[Vec<u8>]) -> Result<Vec<X509Crl>, TlsError> {
    crls.iter()
        .map(|der| {
            X509Crl::from_der(der)
                .map_err(|error| TlsError::Identity(format!("revocation list: {error}")))
        })
        .collect()
}

fn trust_store(roots: &[Vec<u8>]) -> Result<X509Store, TlsError> {
    let mut builder = X509StoreBuilder::new()
        .map_err(|error| TlsError::Identity(format!("trust store: {error}")))?;
    for der in roots {
        builder
            .add_cert(parse_certificate(der)?)
            .map_err(|error| TlsError::Identity(format!("trust anchor: {error}")))?;
    }
    Ok(builder.build())
}

/// The checks of a certificate in the verify callback. Each check that fails raises its event and
/// refuses the certificate.
struct Judge {
    roots: Vec<Vec<u8>>,
    crls: Vec<X509Crl>,
    limit: usize,
    events: Arc<dyn SecurityEvents>,
}

impl Judge {
    /// Judges the certificate that OpenSSL is checking. `preverified` is OpenSSL's own verdict.
    fn check(&self, preverified: bool, context: &mut X509StoreContextRef) -> bool {
        let Some(certificate) = context.current_cert() else {
            return preverified;
        };
        let Ok(der) = certificate.to_der() else {
            return false;
        };
        if der.len() > self.limit {
            self.events.raise(
                SecurityEvent::CertificateSizeExceeded,
                &format!("{} octets", der.len()),
            );
            return false;
        }
        let Ok(now) = Asn1Time::days_from_now(0) else {
            return false;
        };
        if certificate
            .not_after()
            .compare(&now)
            .map_or(true, |o| o.is_lt())
        {
            self.events.raise(SecurityEvent::CertificateExpired, "");
            return false;
        }
        if certificate
            .not_before()
            .compare(&now)
            .map_or(true, |o| o.is_gt())
        {
            return false;
        }
        if !preverified && !self.anchored(context) {
            self.events
                .raise(SecurityEvent::NoCaMatch, "the chain has no trust anchor");
            return false;
        }
        if !preverified {
            return false;
        }
        self.not_revoked(&der, context)
    }

    /// True when the top of the chain is one of the trust anchors.
    fn anchored(&self, context: &X509StoreContextRef) -> bool {
        let Some(chain) = context.chain() else {
            return false;
        };
        let Some(top) = chain.iter().last() else {
            return false;
        };
        top.to_der().is_ok_and(|der| self.roots.contains(&der))
    }

    /// True when the certificate is not revoked by a CRL of its issuer. A certificate whose issuer
    /// has no CRL in the list is refused, as the rustls backend refuses it.
    fn not_revoked(&self, der: &[u8], context: &X509StoreContextRef) -> bool {
        if self.crls.is_empty() {
            return true;
        }
        let Ok(certificate) = parse_certificate(der) else {
            return false;
        };
        let depth = context.error_depth() as usize;
        let issuer = context
            .chain()
            .and_then(|chain| chain.iter().nth(depth + 1));
        // The top of the chain has no issuer in the chain: it is a trust anchor, not checked.
        let Some(issuer) = issuer else {
            return true;
        };
        let Ok(key) = issuer.public_key() else {
            return false;
        };
        for crl in &self.crls {
            if !matches!(crl.verify(&key), Ok(true)) {
                continue;
            }
            return match crl.get_by_cert(&certificate) {
                CrlStatus::Revoked(_) => {
                    self.events.raise(SecurityEvent::CertificateRevoked, "");
                    self.events.raise(SecurityEvent::RevokedSessionClosed, "");
                    false
                }
                CrlStatus::NotRevoked | CrlStatus::RemoveFromCrl(_) => true,
            };
        }
        false
    }
}

impl OpensslBackend {
    /// Changes the renegotiation interval, for the tests: the settings do not allow the short
    /// intervals that a test needs.
    #[cfg(test)]
    pub(crate) fn with_renegotiation(mut self, interval: Option<Duration>) -> Self {
        self.renegotiation = interval;
        self
    }

    /// Secures an outgoing connection, and returns the stream of the backend.
    pub(crate) async fn connect_stream(
        &self,
        tcp: TcpStream,
        host: &str,
    ) -> Result<OpensslStream, TlsError> {
        let configuration = self
            .connector
            .configure()
            .map_err(|error| TlsError::Invalid(error.to_string()))?;
        let ssl: Ssl = configuration
            .into_ssl(host)
            .map_err(|error| TlsError::Invalid(error.to_string()))?;
        let mut stream =
            SslStream::new(ssl, tcp).map_err(|error| TlsError::Invalid(error.to_string()))?;
        Pin::new(&mut stream)
            .connect()
            .await
            .map_err(|error| TlsError::Handshake(error.to_string()))?;
        self.events.raise(SecurityEvent::TlsHandshakeSuccess, "");
        Ok(OpensslStream::new(stream, self.renegotiation))
    }

    /// Secures an accepted connection, and returns the stream of the backend.
    pub(crate) async fn accept_stream(&self, tcp: TcpStream) -> Result<OpensslStream, TlsError> {
        let ssl = Ssl::new(&self.server).map_err(|error| TlsError::Invalid(error.to_string()))?;
        let mut stream =
            SslStream::new(ssl, tcp).map_err(|error| TlsError::Invalid(error.to_string()))?;
        if let Err(error) = Pin::new(&mut stream).accept().await {
            if error.to_string().contains(NO_CLIENT_CERTIFICATE) {
                self.events.raise(SecurityEvent::NoPeerCertificate, "");
            }
            return Err(TlsError::Handshake(error.to_string()));
        }
        self.events.raise(SecurityEvent::TlsHandshakeSuccess, "");
        Ok(OpensslStream::new(stream, self.renegotiation))
    }
}

impl TlsBackend for OpensslBackend {
    fn connect<'a>(&'a self, tcp: TcpStream, host: &'a str) -> Handshake<'a> {
        Box::pin(async move {
            let stream = self.connect_stream(tcp, host).await?;
            Ok(Box::new(stream) as BoxedTransport)
        })
    }

    fn accept<'a>(&'a self, tcp: TcpStream) -> Handshake<'a> {
        Box::pin(async move {
            let stream = self.accept_stream(tcp).await?;
            Ok(Box::new(stream) as BoxedTransport)
        })
    }
}

/// A secured stream of the OpenSSL backend. When the renegotiation interval has passed, the next
/// read or write asks for a renegotiation of the TLS 1.2 session (clause 7.4.5). A TLS 1.3 session
/// is not renegotiated (clause 8.7), so the interval has no effect on it.
pub struct OpensslStream {
    inner: SslStream<TcpStream>,
    interval: Option<Duration>,
    due: Instant,
    writing: bool,
}

impl OpensslStream {
    fn new(inner: SslStream<TcpStream>, interval: Option<Duration>) -> Self {
        let now = Instant::now();
        let due = interval
            .and_then(|interval| now.checked_add(interval))
            .unwrap_or(now);
        Self {
            inner,
            interval,
            due,
            writing: false,
        }
    }

    /// The number of renegotiations of the session so far, on either side.
    pub fn renegotiations(&self) -> i64 {
        openssl_renegotiation::total(self.inner.ssl())
    }

    /// Asks for a renegotiation when the interval has passed. It waits while a write is in progress,
    /// because a write that is retried must see the same bytes.
    fn renegotiate_if_due(&mut self) -> io::Result<()> {
        let Some(interval) = self.interval else {
            return Ok(());
        };
        let now = Instant::now();
        if self.writing || now < self.due {
            return Ok(());
        }
        self.due = now.checked_add(interval).unwrap_or(now);
        if self.inner.ssl().version2() != Some(SslVersion::TLS1_2) {
            return Ok(());
        }
        if openssl_renegotiation::request(self.inner.ssl()) {
            Ok(())
        } else {
            Err(io::Error::other("the session refused a renegotiation"))
        }
    }
}

impl AsyncRead for OpensslStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        if let Err(error) = this.renegotiate_if_due() {
            return Poll::Ready(Err(error));
        }
        Pin::new(&mut this.inner).poll_read(cx, buf)
    }
}

impl AsyncWrite for OpensslStream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        if let Err(error) = this.renegotiate_if_due() {
            return Poll::Ready(Err(error));
        }
        let result = Pin::new(&mut this.inner).poll_write(cx, buf);
        this.writing = result.is_pending();
        result
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_shutdown(cx)
    }
}

#[cfg(test)]
mod tests;
