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
//! TLS 1.2 sessions are renegotiated once the interval of the settings has passed (clause 7.4.5), and
//! TLS 1.3 sessions update their keys once the key update interval has passed (clause 8.4). The
//! `openssl` crate has no wrapper for these calls: they go through `openssl_session`, the only module of
//! igs-iec104 that contains unsafe code (Q-028, approved).
//!
//! This is the default backend. Each mandatory item of the profile has a test that negotiates it
//! (`every_mandatory_item_of_the_profile_is_negotiated`). A peer that does not answer a renegotiation or
//! a KeyUpdate in time ends the session, with its alarm (D-015 and D-016 in PROVENANCE.md).

use std::future::Future;
use std::io;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use openssl::asn1::Asn1Time;
use openssl::nid::Nid;
use openssl::pkey::{Id, PKey};
use openssl::ssl::{
    Ssl, SslConnector, SslContext, SslContextBuilder, SslMethod, SslVerifyMode, SslVersion,
};
use openssl::x509::store::{X509Store, X509StoreBuilder};
use openssl::x509::{CrlStatus, X509Crl, X509Ref, X509StoreContextRef, X509};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpStream;
use tokio::time::{sleep_until, Instant as TokioInstant, Sleep};
use tokio_openssl::SslStream;

use super::openssl_session::{self, KeyUpdates};

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

/// The OpenSSL messages of a handshake in which no signature algorithm is in common: the TLS 1.2 and
/// the TLS 1.3 forms (clause 6.4.4.6).
const NO_COMMON_SIGNATURE: [&str; 2] = [
    "no shared signature algorithms",
    "no suitable signature algorithm",
];

/// The time that a peer has to answer a renegotiation request (clause 7.4.5) or a KeyUpdate (clause 8.4).
/// The standard gives none. The default is t0, the time-out of connection establishment (clause 9.6 of
/// IEC 60870-5-104), so that they are bounded as a connection is (D-015 and D-016 in PROVENANCE.md).
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(30);

/// The OpenSSL backend: the client and server contexts of one station.
pub struct OpensslBackend {
    connector: SslConnector,
    server: SslContext,
    events: Arc<dyn SecurityEvents>,
    renegotiation: Option<Duration>,
    key_update: Option<Duration>,
    response: Duration,
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
            key_update: settings.key_update(),
            response: RESPONSE_TIMEOUT,
        })
    }
}

/// The judge of the certificates of one context: the settings it checks against.
fn judge(settings: &TlsSettings, events: &Arc<dyn SecurityEvents>) -> Result<Judge, TlsError> {
    Ok(Judge {
        roots: settings.trust().roots().to_vec(),
        crls: parse_crls(settings.revocation())?,
        limit: settings.max_certificate_size(),
        rsa_1024: settings.allows_rsa_1024(),
        events: Arc::clone(events),
    })
}

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
/// The verify result of a signature that does not verify: `X509_V_ERR_CERT_SIGNATURE_FAILURE` in the
/// installed `x509_vfy.h`.
const CERT_SIGNATURE_FAILURE: i32 = 7;
/// The verify results of a key below OpenSSL's security level: `X509_V_ERR_EE_KEY_TOO_SMALL` and
/// `X509_V_ERR_CA_KEY_TOO_SMALL` in the installed `x509_vfy.h`. The judge applies the policy of clause
/// 7.4.3 to these keys instead.
const KEY_TOO_SMALL: [i32; 2] = [66, 67];

struct Judge {
    roots: Vec<Vec<u8>>,
    crls: Vec<X509Crl>,
    limit: usize,
    rsa_1024: bool,
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
        if !self.key_and_hash_allowed(certificate) {
            return false;
        }
        let error = context.error().as_raw();
        // OpenSSL refuses a key below its security level. The policy above has judged that key.
        let preverified = preverified || KEY_TOO_SMALL.contains(&error);
        if !preverified && error == CERT_SIGNATURE_FAILURE {
            self.events.raise(
                SecurityEvent::SignatureNotVerified,
                "the certificate signature does not verify",
            );
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
        self.not_revoked(&der, context, &now)
    }

    /// The key length and the hash of a certificate (clause 7.4.3). An RSA key below 2 048 bits is
    /// refused, unless the policy allows 1 024 bits and the key has at least 1 024 bits: then the
    /// warning MinimumKeyLength is raised. A key below 1 024 bits raises ShortKey as well. A
    /// certificate signed with SHA-1 raises the warning DeprecatedHash and is refused (D-018).
    fn key_and_hash_allowed(&self, certificate: &X509Ref) -> bool {
        let Ok(key) = certificate.public_key() else {
            return false;
        };
        if key.id() == Id::RSA {
            let bits = key.bits();
            if bits < 2048 && !(self.rsa_1024 && bits >= 1024) {
                self.events.raise(
                    SecurityEvent::ShortRsaKey,
                    &format!("an RSA key of {bits} bits"),
                );
                if bits < 1024 {
                    self.events.raise(
                        SecurityEvent::ShortKey,
                        &format!("an RSA key of {bits} bits"),
                    );
                }
                return false;
            }
            if bits < 2048 {
                self.events.raise(
                    SecurityEvent::MinimumKeyLength,
                    &format!("an RSA key of {bits} bits"),
                );
            }
        }
        let algorithm = certificate.signature_algorithm().object().nid();
        if algorithm == Nid::SHA1WITHRSA
            || algorithm == Nid::SHA1WITHRSAENCRYPTION
            || algorithm == Nid::ECDSA_WITH_SHA1
        {
            self.events.raise(
                SecurityEvent::DeprecatedHash,
                "a certificate signed with SHA-1",
            );
            return false;
        }
        true
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

    /// True when the certificate is not revoked by a CRL of its issuer (clause 6.4.4.4). A CRL that has
    /// expired, or the lack of a CRL of the issuer, is a warning, and the session goes on (clause
    /// 6.4.4.4.2, D-017).
    fn not_revoked(&self, der: &[u8], context: &X509StoreContextRef, now: &Asn1Time) -> bool {
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
        let Some(crl) = self
            .crls
            .iter()
            .find(|crl| matches!(crl.verify(&key), Ok(true)))
        else {
            self.events
                .raise(SecurityEvent::NoCrl, "no CRL of the issuer is configured");
            return true;
        };
        if crl
            .next_update()
            .is_some_and(|next| next.compare(now).map_or(true, |o| o.is_lt()))
        {
            self.events.raise(
                SecurityEvent::CrlExpired,
                "the CRL of the issuer has expired",
            );
        }
        match crl.get_by_cert(&certificate) {
            CrlStatus::Revoked(_) => {
                self.events.raise(SecurityEvent::CertificateRevoked, "");
                self.events.raise(SecurityEvent::RevokedSessionClosed, "");
                false
            }
            CrlStatus::NotRevoked | CrlStatus::RemoveFromCrl(_) => true,
        }
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

    /// Changes the time that a peer has to answer a renegotiation, for the tests.
    #[cfg(test)]
    pub(crate) fn with_response(mut self, response: Duration) -> Self {
        self.response = response;
        self
    }

    /// Changes the interval of the key update, for the tests.
    #[cfg(test)]
    pub(crate) fn with_key_update(mut self, interval: Option<Duration>) -> Self {
        self.key_update = interval;
        self
    }

    /// Secures an outgoing connection, and returns the stream of the backend.
    /// The timers of the streams that this backend secures.
    fn timers(&self) -> Timers {
        Timers {
            interval: self.renegotiation,
            key_update: self.key_update,
            response: self.response,
            events: Arc::clone(&self.events),
        }
    }

    /// Raises the alarms of a failed handshake that OpenSSL reports by its message: the peer sent no
    /// certificate (clause 6.4.3), or no signature algorithm is in common (clause 6.4.4.6).
    fn raise_handshake_failure(&self, error: &openssl::ssl::Error) {
        let message = error.to_string();
        if message.contains(NO_CLIENT_CERTIFICATE) {
            self.events.raise(SecurityEvent::NoPeerCertificate, "");
        }
        if NO_COMMON_SIGNATURE
            .iter()
            .any(|reason| message.contains(reason))
        {
            self.events.raise(
                SecurityEvent::SignatureAlgorithmNotSupported,
                "no signature algorithm in common",
            );
        }
    }

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
        if let Err(error) = Pin::new(&mut stream).connect().await {
            self.raise_handshake_failure(&error);
            return Err(TlsError::Handshake(error.to_string()));
        }
        self.events.raise(SecurityEvent::TlsHandshakeSuccess, "");
        Ok(OpensslStream::new(stream, self.timers()))
    }

    /// Secures an accepted connection, and returns the stream of the backend.
    pub(crate) async fn accept_stream(&self, tcp: TcpStream) -> Result<OpensslStream, TlsError> {
        let ssl = Ssl::new(&self.server).map_err(|error| TlsError::Invalid(error.to_string()))?;
        let mut stream =
            SslStream::new(ssl, tcp).map_err(|error| TlsError::Invalid(error.to_string()))?;
        if let Err(error) = Pin::new(&mut stream).accept().await {
            self.raise_handshake_failure(&error);
            return Err(TlsError::Handshake(error.to_string()));
        }
        self.events.raise(SecurityEvent::TlsHandshakeSuccess, "");
        Ok(OpensslStream::new(stream, self.timers()))
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
/// is not renegotiated (clause 8.7), so the interval has no effect on it. A renegotiation that the
/// peer does not answer in time ends the session, with the alarm TLS_NO_RENEG (D-015).
pub struct OpensslStream {
    inner: SslStream<TcpStream>,
    /// The KeyUpdate messages of the session, counted by its message callback. It is declared after
    /// the session, so that the session is freed before the counters.
    key_updates: Arc<KeyUpdates>,
    interval: Option<Duration>,
    key_interval: Option<Duration>,
    response: Duration,
    events: Arc<dyn SecurityEvents>,
    due: Instant,
    key_due: Instant,
    /// The time by which the peer must answer the renegotiation that this side asked for (clause 7.4.5).
    answer: Option<Instant>,
    /// The KeyUpdate that this side asked for, while the peer has not answered it (clause 8.4).
    key_request: Option<KeyRequest>,
    /// The KeyUpdate messages of the peer that have been answered.
    answered_peer_requests: u32,
    /// Wakes the stream at its next deadline when no read or write does: an idle session is then
    /// still renegotiated or updated, and a request that the peer does not answer still ends it.
    timer: Option<Pin<Box<Sleep>>>,
    writing: bool,
}

/// A KeyUpdate that this side asked for: the counts of the KeyUpdate messages when it was asked, and
/// the time by which the peer must answer it. The time is set once the message is written.
#[derive(Clone, Copy, Debug)]
struct KeyRequest {
    written: u32,
    answered: u32,
    by: Option<Instant>,
}

/// True when the peer refused a renegotiation with a no_renegotiation alert (RFC 5746): OpenSSL then
/// ends the read with the error "no renegotiation".
fn refused_renegotiation(error: &io::Error) -> bool {
    error
        .get_ref()
        .and_then(|inner| inner.downcast_ref::<openssl::ssl::Error>())
        .and_then(|error| error.ssl_error())
        .is_some_and(|stack| {
            stack
                .errors()
                .iter()
                .any(|error| error.reason() == Some("no renegotiation"))
        })
}

/// The timers of a session, and the events that they raise.
struct Timers {
    interval: Option<Duration>,
    key_update: Option<Duration>,
    response: Duration,
    events: Arc<dyn SecurityEvents>,
}

impl OpensslStream {
    fn new(inner: SslStream<TcpStream>, timers: Timers) -> Self {
        let now = Instant::now();
        // The renegotiation applies to TLS 1.2 sessions, and the key update to TLS 1.3 sessions.
        let version = inner.ssl().version2();
        let (interval, key_interval) = if version == Some(SslVersion::TLS1_2) {
            (timers.interval, None)
        } else if version == Some(SslVersion::TLS1_3) {
            (None, timers.key_update)
        } else {
            (None, None)
        };
        let key_updates = Arc::new(KeyUpdates::default());
        openssl_session::observe_key_updates(inner.ssl(), &key_updates);
        let due = interval
            .and_then(|interval| now.checked_add(interval))
            .unwrap_or(now);
        let key_due = key_interval
            .and_then(|interval| now.checked_add(interval))
            .unwrap_or(now);
        Self {
            inner,
            key_updates,
            interval,
            key_interval,
            response: timers.response,
            events: timers.events,
            due,
            key_due,
            answer: None,
            key_request: None,
            answered_peer_requests: 0,
            timer: None,
            writing: false,
        }
    }

    /// The number of renegotiations that have started on the session so far, on either side.
    pub fn renegotiations(&self) -> i64 {
        openssl_session::total(self.inner.ssl())
    }

    /// The KeyUpdate messages of the session: the ones written, the answers received, and the requests
    /// of the peer.
    pub fn key_updates(&self) -> &KeyUpdates {
        &self.key_updates
    }

    /// Runs the renegotiation timer of a TLS 1.2 session. A renegotiation is asked for once the interval
    /// has passed, and none is asked for while another is unanswered. A peer that does not answer in
    /// time ends the session, and the alarm of a missed renegotiation is raised (clause 7.4.5).
    fn service_renegotiation(&mut self) -> io::Result<()> {
        let now = Instant::now();
        if let Some(by) = self.answer {
            if !openssl_session::pending(self.inner.ssl()) {
                self.answer = None;
                if let Some(interval) = self.interval {
                    self.due = now.checked_add(interval).unwrap_or(now);
                }
            } else if now >= by {
                self.answer = None;
                self.events.raise(
                    SecurityEvent::RenegotiationExpired,
                    "the peer did not renegotiate in time",
                );
                return Err(io::Error::other("the peer did not renegotiate in time"));
            }
        }
        let Some(interval) = self.interval else {
            return Ok(());
        };
        if self.writing || self.answer.is_some() || now < self.due {
            return Ok(());
        }
        self.due = now.checked_add(interval).unwrap_or(now);
        if !openssl_session::request(self.inner.ssl()) {
            return Err(io::Error::other("the session refused a renegotiation"));
        }
        self.answer = Some(now.checked_add(self.response).unwrap_or(now));
        Ok(())
    }

    /// Runs the key update of a TLS 1.3 session (clause 8.4). When the interval has passed, a KeyUpdate
    /// that asks the peer to update its keys too is queued and written. The peer must answer it within
    /// the response time, counted from the write. An unanswered KeyUpdate ends the session, with the
    /// alarm TLS_NO_SK_UPDATE.
    fn service_key_update(&mut self, cx: &mut Context<'_>) -> io::Result<()> {
        let Some(interval) = self.key_interval else {
            return Ok(());
        };
        let now = Instant::now();
        if let Some(request) = self.key_request {
            if self.key_updates.answered() > request.answered {
                self.key_request = None;
                self.key_due = now.checked_add(interval).unwrap_or(now);
                return Ok(());
            }
            if let Some(by) = request.by {
                if now >= by {
                    self.key_request = None;
                    self.events.raise(
                        SecurityEvent::KeyUpdateExpired,
                        "the peer did not answer the key update in time",
                    );
                    return Err(io::Error::other(
                        "the peer did not answer the key update in time",
                    ));
                }
                return Ok(());
            }
        } else {
            if self.writing || now < self.key_due {
                return Ok(());
            }
            self.key_due = now.checked_add(interval).unwrap_or(now);
            if !openssl_session::request_key_update(self.inner.ssl()) {
                return Err(io::Error::other("the session refused a key update"));
            }
            self.key_request = Some(KeyRequest {
                written: self.key_updates.written(),
                answered: self.key_updates.answered(),
                by: None,
            });
        }
        // The queued KeyUpdate is written by a handshake call. It waits while a write is in progress.
        if self.writing {
            return Ok(());
        }
        self.flush(cx)?;
        if let Some(request) = self.key_request.as_mut() {
            if request.by.is_none() && self.key_updates.written() > request.written {
                request.by = Some(now.checked_add(self.response).unwrap_or(now));
            }
        }
        Ok(())
    }

    /// Runs a handshake call of the session. It writes the KeyUpdate messages that OpenSSL has queued,
    /// without waiting for the peer.
    fn flush(&mut self, cx: &mut Context<'_>) -> io::Result<()> {
        match Pin::new(&mut self.inner).poll_do_handshake(cx) {
            Poll::Ready(Err(error)) => Err(io::Error::other(error)),
            Poll::Ready(Ok(())) | Poll::Pending => Ok(()),
        }
    }

    /// Writes the answer to a KeyUpdate of the peer, which OpenSSL queues when it reads the request
    /// (clause 8.4). It runs after a read, while no write is in progress.
    fn answer_key_updates(&mut self, cx: &mut Context<'_>) -> io::Result<()> {
        let requested = self.key_updates.requested_by_peer();
        if requested == self.answered_peer_requests || self.writing {
            return Ok(());
        }
        self.answered_peer_requests = requested;
        self.flush(cx)
    }

    /// Raises the alarm when the peer refused a request of this side with a no_renegotiation alert
    /// instead of a ClientHello: the request is then unanswered (clause 7.4.5).
    fn note_refusal(&mut self, error: &io::Error) {
        if self.answer.is_some() && refused_renegotiation(error) {
            self.answer = None;
            self.events.raise(
                SecurityEvent::RenegotiationExpired,
                "the peer refused the renegotiation",
            );
        }
    }

    /// Sets the timer to the next deadline of the session, or stops it while a write is in progress: the
    /// write wakes the stream itself.
    fn arm(&mut self) {
        let renegotiation = self.answer.or_else(|| self.interval.map(|_| self.due));
        let key_update = match self.key_request {
            Some(request) => request.by,
            None => self.key_interval.map(|_| self.key_due),
        };
        let deadline = if self.writing {
            None
        } else {
            [renegotiation, key_update].into_iter().flatten().min()
        };
        let Some(deadline) = deadline else {
            self.timer = None;
            return;
        };
        let deadline = TokioInstant::from_std(deadline);
        match self.timer.as_mut() {
            Some(timer) => timer.as_mut().reset(deadline),
            None => self.timer = Some(Box::pin(sleep_until(deadline))),
        }
    }

    /// Runs the timers, and registers the waker of the next deadline. A deadline that has already
    /// passed is handled before the read or write goes on.
    fn poll_timers(&mut self, cx: &mut Context<'_>) -> io::Result<()> {
        for _ in 0..2 {
            self.service_renegotiation()?;
            self.service_key_update(cx)?;
            self.arm();
            let Some(timer) = self.timer.as_mut() else {
                break;
            };
            if timer.as_mut().poll(cx).is_pending() {
                break;
            }
        }
        Ok(())
    }
}

impl AsyncRead for OpensslStream {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        if let Err(error) = this.poll_timers(cx) {
            return Poll::Ready(Err(error));
        }
        let result = Pin::new(&mut this.inner).poll_read(cx, buf);
        if let Poll::Ready(Err(error)) = &result {
            this.note_refusal(error);
        }
        if matches!(result, Poll::Ready(Ok(()))) {
            if let Err(error) = this.answer_key_updates(cx) {
                return Poll::Ready(Err(error));
            }
        }
        result
    }
}

impl AsyncWrite for OpensslStream {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let this = self.get_mut();
        if let Err(error) = this.poll_timers(cx) {
            return Poll::Ready(Err(error));
        }
        let result = Pin::new(&mut this.inner).poll_write(cx, buf);
        this.writing = result.is_pending();
        if let Poll::Ready(Err(error)) = &result {
            this.note_refusal(error);
        }
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
