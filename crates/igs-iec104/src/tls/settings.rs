// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! The settings of a TLS connection: the identity of the station, its trust anchors, the
//! revocation lists and the policy. The certificates are held as DER, so that the settings do
//! not depend on a backend; a backend reads them.

use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use super::events::{SecurityEvent, SecurityEvents};
use super::TlsError;

/// The smallest certificate size that a conformant implementation handles (clause 6.4.2).
pub const MIN_CERTIFICATE_SIZE: usize = 8192;

/// The shortest interval between two renegotiations of a TLS 1.2 session (clause 7.4.5).
pub const MIN_RENEGOTIATION: Duration = Duration::from_secs(10 * 60);
/// The longest interval between two renegotiations: a long session is renegotiated at least once in
/// 24 hours, to check the certificates (clause 7.4.5).
pub const MAX_RENEGOTIATION: Duration = Duration::from_secs(24 * 60 * 60);
/// The interval that the profile suggests (clause 7.4.5).
pub const DEFAULT_RENEGOTIATION: Duration = Duration::from_secs(12 * 60 * 60);

/// The local identity: the certificate chain, the leaf first, and the private key. Both are DER.
#[derive(Clone)]
pub struct Identity {
    chain: Vec<Vec<u8>>,
    key: Vec<u8>,
}

impl Identity {
    /// Builds an identity. Raises [`SecurityEvent::NoLocalCertificate`] and fails when the chain
    /// or the key is empty (clause 6.4.3).
    pub fn new(
        chain: Vec<Vec<u8>>,
        key: Vec<u8>,
        events: &dyn SecurityEvents,
    ) -> Result<Self, TlsError> {
        if chain.is_empty() || key.is_empty() {
            events.raise(
                SecurityEvent::NoLocalCertificate,
                "no certificate or no key",
            );
            return Err(TlsError::Identity(
                "the identity has no certificate or no key".into(),
            ));
        }
        Ok(Self { chain, key })
    }

    /// The certificate chain, the leaf first, in DER.
    pub fn chain(&self) -> &[Vec<u8>] {
        &self.chain
    }

    /// The private key, in DER.
    pub fn key(&self) -> &[u8] {
        &self.key
    }
}

// The key is secret: the debug output shows its length only.
impl fmt::Debug for Identity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Identity")
            .field("certificates", &self.chain.len())
            .field("key", &format_args!("{} octets", self.key.len()))
            .finish()
    }
}

/// The trust anchors (root certificates) of the station, in DER. The profile requires support for
/// at least five of them (clause 6.4.1); this type does not limit their number.
#[derive(Clone, Debug)]
pub struct TrustAnchors {
    roots: Vec<Vec<u8>>,
}

impl TrustAnchors {
    /// Builds the trust anchors. Fails when there is none, since no peer could be verified.
    pub fn new(roots: Vec<Vec<u8>>) -> Result<Self, TlsError> {
        if roots.is_empty() {
            return Err(TlsError::Invalid("no trust anchor".into()));
        }
        Ok(Self { roots })
    }

    /// The root certificates, in DER.
    pub fn roots(&self) -> &[Vec<u8>] {
        &self.roots
    }
}

/// The settings of a connection, checked against the profile by [`TlsSettings::validate`].
#[derive(Clone)]
pub struct TlsSettings {
    identity: Identity,
    trust: TrustAnchors,
    revocation: Vec<Vec<u8>>,
    tls13: bool,
    max_certificate_size: usize,
    renegotiation: Option<Duration>,
    events: Arc<dyn SecurityEvents>,
}

impl TlsSettings {
    /// The settings with the defaults of the profile: TLS 1.2 and TLS 1.3 enabled, certificates
    /// up to [`MIN_CERTIFICATE_SIZE`] octets, and no revocation list.
    pub fn new(identity: Identity, trust: TrustAnchors, events: Arc<dyn SecurityEvents>) -> Self {
        Self {
            identity,
            trust,
            revocation: Vec::new(),
            tls13: true,
            max_certificate_size: MIN_CERTIFICATE_SIZE,
            renegotiation: Some(DEFAULT_RENEGOTIATION),
            events,
        }
    }

    /// Enables or disables TLS 1.3. TLS 1.2 is mandatory (clause 10.3) and cannot be disabled.
    #[must_use]
    pub fn with_tls13(mut self, enabled: bool) -> Self {
        self.tls13 = enabled;
        self
    }

    /// Sets the certificate revocation lists, in DER (clause 6.4.4.4.2).
    #[must_use]
    pub fn with_revocation(mut self, crls: Vec<Vec<u8>>) -> Self {
        self.revocation = crls;
        self
    }

    /// Sets the largest certificate that is handled, in octets. Below
    /// [`MIN_CERTIFICATE_SIZE`] the settings are refused by [`TlsSettings::validate`].
    #[must_use]
    pub fn with_max_certificate_size(mut self, octets: usize) -> Self {
        self.max_certificate_size = octets;
        self
    }

    /// Sets the interval between two renegotiations of a TLS 1.2 session, or `None` for no
    /// renegotiation. The profile allows 10 minutes to 24 hours and suggests 12 hours (clause
    /// 7.4.5). The OpenSSL backend renegotiates; the rustls backend does not (see its module).
    #[must_use]
    pub fn with_renegotiation(mut self, interval: Option<Duration>) -> Self {
        self.renegotiation = interval;
        self
    }

    /// Checks the settings against the profile. The backend calls it before it builds anything.
    pub fn validate(&self) -> Result<(), TlsError> {
        if self.max_certificate_size < MIN_CERTIFICATE_SIZE {
            return Err(TlsError::Invalid(format!(
                "the certificate size limit must be at least {MIN_CERTIFICATE_SIZE} octets"
            )));
        }
        if let Some(interval) = self.renegotiation {
            if !(MIN_RENEGOTIATION..=MAX_RENEGOTIATION).contains(&interval) {
                return Err(TlsError::Invalid(
                    "the renegotiation interval is 10 minutes to 24 hours".into(),
                ));
            }
        }
        Ok(())
    }

    /// The local identity.
    pub fn identity(&self) -> &Identity {
        &self.identity
    }

    /// The trust anchors.
    pub fn trust(&self) -> &TrustAnchors {
        &self.trust
    }

    /// The revocation lists, in DER.
    pub fn revocation(&self) -> &[Vec<u8>] {
        &self.revocation
    }

    /// True when TLS 1.3 is enabled.
    pub fn tls13(&self) -> bool {
        self.tls13
    }

    /// The largest certificate that is handled, in octets.
    pub fn max_certificate_size(&self) -> usize {
        self.max_certificate_size
    }

    /// The interval between two renegotiations of a TLS 1.2 session, if any.
    pub fn renegotiation(&self) -> Option<Duration> {
        self.renegotiation
    }

    /// Where the security events go.
    pub fn events(&self) -> &Arc<dyn SecurityEvents> {
        &self.events
    }
}

#[cfg(test)]
mod tests {
    use super::super::events::Discard;
    use super::*;

    #[test]
    fn an_identity_without_a_key_is_refused_and_raises_the_event() {
        let events = Arc::new(Recorder::default());
        let error = Identity::new(vec![vec![1]], Vec::new(), events.as_ref());
        assert!(error.is_err());
        assert_eq!(events.seen(), vec![SecurityEvent::NoLocalCertificate]);
    }

    #[test]
    fn the_debug_output_does_not_show_the_key() {
        let identity = Identity::new(vec![vec![1, 2]], vec![9, 9, 9], &Discard).expect("valid");
        let shown = format!("{identity:?}");
        assert!(shown.contains("3 octets"));
        assert!(!shown.contains("9, 9"));
    }

    #[test]
    fn the_certificate_size_below_the_profile_minimum_is_refused() {
        let identity = Identity::new(vec![vec![1]], vec![2], &Discard).expect("valid");
        let trust = TrustAnchors::new(vec![vec![3]]).expect("one anchor");
        let settings = TlsSettings::new(identity, trust, Arc::new(Discard));
        assert!(settings.validate().is_ok());
        assert!(settings
            .clone()
            .with_max_certificate_size(8191)
            .validate()
            .is_err());
        assert!(settings.with_max_certificate_size(8192).validate().is_ok());
    }

    #[test]
    fn there_must_be_a_trust_anchor() {
        assert!(TrustAnchors::new(Vec::new()).is_err());
    }

    /// Keeps the events it receives, for the assertions.
    #[derive(Default)]
    struct Recorder(std::sync::Mutex<Vec<SecurityEvent>>);

    impl Recorder {
        fn seen(&self) -> Vec<SecurityEvent> {
            self.0
                .lock()
                .map(|events| events.clone())
                .unwrap_or_default()
        }
    }

    impl SecurityEvents for Recorder {
        fn raise(&self, event: SecurityEvent, _detail: &str) {
            if let Ok(mut events) = self.0.lock() {
                events.push(event);
            }
        }
    }
}
