// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! The security events of IEC 62351-3 (its Annex A): what an implementation raises when a TLS
//! handshake or a certificate check goes wrong. The mnemonics, severities and identifiers are
//! data; the descriptions are ours.

use std::fmt;

/// The severity of a security event, as the standard grades it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    /// Information: the step succeeded.
    Notice,
    /// Something is weak or deprecated, but the session may go on.
    Warning,
    /// The session is refused or terminated.
    Alarm,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Notice => "notice",
            Self::Warning => "warning",
            Self::Alarm => "alarm",
        })
    }
}

/// A security event of Annex A. The clause in each variant's documentation is where the
/// standard defines the event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SecurityEvent {
    /// The TLS handshake succeeded (clause 6.1).
    TlsHandshakeSuccess,
    /// A deprecated TLS version was proposed while it is disabled (clause 6.2).
    DeprecatedVersion,
    /// A weak TLS version was proposed while it is enabled (clause 6.2).
    WeakVersion,
    /// A disallowed TLS version was proposed (clause 6.2).
    DisallowedVersion,
    /// The TLS version changed in an ongoing session (clause 6.2).
    VersionChange,
    /// The peer did not provide a certificate (clause 6.4.3).
    NoPeerCertificate,
    /// The local certificate or its key cannot be used (clause 6.4.3).
    NoLocalCertificate,
    /// A session was terminated because of a revoked certificate (clause 6.4.4.4).
    RevokedSessionClosed,
    /// A disallowed cipher suite was proposed (clause 7.3).
    DisallowedCipherSuite,
    /// A session ID expired, and the session was set up again in full (clause 7.4.4).
    SessionIdExpired,
    /// The session was not renegotiated within its interval (clause 7.4.5).
    RenegotiationExpired,
    /// The initial handshake did not signal secure renegotiation (clause 7.5.2).
    NoSecureRenegotiationInitial,
    /// A renegotiated handshake did not use secure renegotiation (clause 7.5.2).
    NoSecureRenegotiationRenegotiated,
    /// The server has no certificate for the CA that the client signalled (clause 7.5.3).
    NoTrustedCaMatchServer,
    /// The client sent no signature algorithm extension (clause 7.5.4).
    NoSignatureAlgorithmExtension,
    /// A deprecated signature algorithm was signalled (clause 7.5.4).
    DeprecatedSignatureAlgorithm,
    /// A CBC cipher suite was negotiated without encrypt-then-MAC (clause 7.5.7).
    NoEncryptThenMac,
    /// Only non-ephemeral PSK modes were proposed (clause 8.3.2).
    NoEphemeralPskMode,
    /// The session key was not updated within its interval (clause 8.4).
    KeyUpdateExpired,
    /// The server has no certificate for the CA that the peer signalled (clause 8.8.8).
    NoTrustedCaMatchPeer,
    /// Early data was received in the handshake and ignored (clause 8.8.11).
    EarlyDataIgnored,
    /// The certificate is larger than the supported size (clause 6.4.2).
    CertificateSizeExceeded,
    /// No CA certificate matches the certificate chain (clause 6.4.4.2).
    NoCaMatch,
    /// The certificate is not in the list of trusted certificates (clause 6.4.4.3).
    NoTrustedCertificateMatch,
    /// The certificate is revoked (clause 6.4.4.4).
    CertificateRevoked,
    /// No local CRL is accessible (clause 6.4.4.4.2).
    NoCrl,
    /// The local CRL has expired (clause 6.4.4.4.2).
    CrlExpired,
    /// The OCSP response has expired (clause 6.4.4.4.3).
    OcspResponseExpired,
    /// The OCSP responder is not accessible (clause 6.4.4.4.3).
    OcspResponderUnavailable,
    /// The certificate has expired (clause 6.4.4.5).
    CertificateExpired,
    /// The signature algorithms of the certificate are not supported (clause 6.4.4.6).
    SignatureAlgorithmNotSupported,
    /// The signature of the certificate could not be verified (clause 6.4.4.6).
    SignatureNotVerified,
    /// The RSA key is shorter than 2 048 bits (clause 7.4.3).
    ShortRsaKey,
    /// The RSA key is 1 024 bits, allowed by configuration (clause 7.4.3).
    MinimumKeyLength,
    /// The RSA key is shorter than 1 024 bits (clause 7.4.3).
    ShortKey,
    /// A deprecated hash algorithm is used (clause 7.4.3).
    DeprecatedHash,
}

/// The data of one event: its mnemonic, severity, identifier and clause.
struct Row {
    mnemonic: &'static str,
    severity: Severity,
    group: u8,
    number: u8,
    clause: &'static str,
}

impl SecurityEvent {
    /// Every event, in the order of Annex A.
    pub const ALL: [SecurityEvent; 36] = [
        Self::TlsHandshakeSuccess,
        Self::DeprecatedVersion,
        Self::WeakVersion,
        Self::DisallowedVersion,
        Self::VersionChange,
        Self::NoPeerCertificate,
        Self::NoLocalCertificate,
        Self::RevokedSessionClosed,
        Self::DisallowedCipherSuite,
        Self::SessionIdExpired,
        Self::RenegotiationExpired,
        Self::NoSecureRenegotiationInitial,
        Self::NoSecureRenegotiationRenegotiated,
        Self::NoTrustedCaMatchServer,
        Self::NoSignatureAlgorithmExtension,
        Self::DeprecatedSignatureAlgorithm,
        Self::NoEncryptThenMac,
        Self::NoEphemeralPskMode,
        Self::KeyUpdateExpired,
        Self::NoTrustedCaMatchPeer,
        Self::EarlyDataIgnored,
        Self::CertificateSizeExceeded,
        Self::NoCaMatch,
        Self::NoTrustedCertificateMatch,
        Self::CertificateRevoked,
        Self::NoCrl,
        Self::CrlExpired,
        Self::OcspResponseExpired,
        Self::OcspResponderUnavailable,
        Self::CertificateExpired,
        Self::SignatureAlgorithmNotSupported,
        Self::SignatureNotVerified,
        Self::ShortRsaKey,
        Self::MinimumKeyLength,
        Self::ShortKey,
        Self::DeprecatedHash,
    ];

    /// The mnemonic of the event, as Annex A prints it.
    pub fn mnemonic(self) -> &'static str {
        self.row().mnemonic
    }

    /// The severity of the event.
    pub fn severity(self) -> Severity {
        self.row().severity
    }

    /// The clause of IEC 62351-3 that defines the event.
    pub fn clause(self) -> &'static str {
        self.row().clause
    }

    /// The identifier of the event, `IEC 62351-3:<group>.<number>`.
    pub fn identifier(self) -> String {
        let row = self.row();
        format!("IEC 62351-3:{}.{}", row.group, row.number)
    }

    fn row(self) -> Row {
        let (mnemonic, severity, group, number, clause) = match self {
            Self::TlsHandshakeSuccess => ("TLS_HS_SUCCESS", Severity::Notice, 1, 1, "6.1"),
            Self::DeprecatedVersion => ("TLS_DEPRECATED_VERSION", Severity::Alarm, 1, 2, "6.2"),
            Self::WeakVersion => ("TLS_WEAK_VERSION", Severity::Warning, 1, 3, "6.2"),
            Self::DisallowedVersion => ("TLS_DISALLOWED_VERSION", Severity::Alarm, 1, 4, "6.2"),
            Self::VersionChange => ("TLS_VERSION_CHANGE", Severity::Alarm, 1, 5, "6.2"),
            Self::NoPeerCertificate => ("TLS_NO_PEER_CERT", Severity::Alarm, 1, 6, "6.4.3"),
            Self::NoLocalCertificate => ("TLS_NO_LOCAL_CERT", Severity::Alarm, 1, 7, "6.4.3"),
            Self::RevokedSessionClosed => {
                ("TLS_SESSION_CLOSED-REV", Severity::Alarm, 1, 8, "6.4.4.4")
            }
            Self::DisallowedCipherSuite => {
                ("TLS_DISALLOWED_CIPHER", Severity::Warning, 1, 9, "7.3")
            }
            Self::SessionIdExpired => (
                "TLS_SESSIONID_EXPIRED_FULL_HS",
                Severity::Warning,
                1,
                10,
                "7.4.4",
            ),
            Self::RenegotiationExpired => ("TLS_NO_RENEG", Severity::Alarm, 1, 11, "7.4.5"),
            Self::NoSecureRenegotiationInitial => {
                ("TLS_NO_RENEG_SIG", Severity::Warning, 1, 12, "7.5.2")
            }
            Self::NoSecureRenegotiationRenegotiated => {
                ("TLS_NO_RENEG_TICKET", Severity::Alarm, 1, 13, "7.5.2")
            }
            Self::NoTrustedCaMatchServer => {
                ("TLS_NO_TR_CA_MATCH_S", Severity::Alarm, 1, 14, "7.5.3")
            }
            Self::NoSignatureAlgorithmExtension => {
                ("TLS_NO-SIG_ALGO_EXT", Severity::Warning, 1, 15, "7.5.4")
            }
            Self::DeprecatedSignatureAlgorithm => {
                ("TLS_DEP_SIG_ALGO", Severity::Warning, 1, 16, "7.5.4")
            }
            // The mnemonic of this row is printed as the standard has it; the table pairs it
            // with the PSK clause, so Q-020 asks the maintainers to confirm the pairing.
            Self::NoEncryptThenMac => ("TLS_NO_EPSK_MODE", Severity::Alarm, 1, 17, "7.5.7"),
            Self::NoEphemeralPskMode => {
                ("TLS_NO_ENCRYPT-THEN-MAC", Severity::Warning, 1, 18, "8.3.2")
            }
            Self::KeyUpdateExpired => ("TLS_NO_SK_UPDATE", Severity::Alarm, 1, 19, "8.4"),
            Self::NoTrustedCaMatchPeer => {
                ("TLS_NO_TR_CA_MATCH_SC", Severity::Alarm, 1, 20, "8.8.8")
            }
            Self::EarlyDataIgnored => ("TLS_EARLY-DATA", Severity::Warning, 1, 21, "8.8.11"),
            Self::CertificateSizeExceeded => {
                ("TLS_CERT_SIZE_MISMATCH", Severity::Alarm, 2, 1, "6.4.2")
            }
            Self::NoCaMatch => ("TLS_NO_CA_MATCH", Severity::Alarm, 2, 2, "6.4.4.2"),
            Self::NoTrustedCertificateMatch => (
                "TLS_NO_TRUSTED_CERT_MATCH",
                Severity::Alarm,
                2,
                3,
                "6.4.4.3",
            ),
            Self::CertificateRevoked => ("TLS_CERT_REVOKED", Severity::Alarm, 2, 4, "6.4.4.4"),
            Self::NoCrl => ("TLS_NO_CRL", Severity::Warning, 2, 5, "6.4.4.4.2"),
            Self::CrlExpired => ("TLS_CRL_EXP", Severity::Warning, 2, 6, "6.4.4.4.2"),
            Self::OcspResponseExpired => ("TLS_OCSP_RES_EXP", Severity::Warning, 2, 7, "6.4.4.4.3"),
            // The identifiers 2.8 of this row and of CertificateExpired are both printed in the
            // standard; Q-021 asks which one is right.
            Self::OcspResponderUnavailable => (
                "TLS_OCSP_RESP_UNAVAIL",
                Severity::Warning,
                2,
                8,
                "6.4.4.4.3",
            ),
            Self::CertificateExpired => ("TLS_CERT_EXP", Severity::Alarm, 2, 8, "6.4.4.5"),
            Self::SignatureAlgorithmNotSupported => {
                ("TLS_SIG_ALG_MISMATCH", Severity::Alarm, 2, 9, "6.4.4.6")
            }
            Self::SignatureNotVerified => ("TLS_SIG_V_FAILED", Severity::Alarm, 2, 10, "6.4.4.6"),
            Self::ShortRsaKey => ("TLS_SHORT_RSA_KEY", Severity::Alarm, 2, 11, "7.4.3"),
            Self::MinimumKeyLength => ("TLS_MIN_KEY", Severity::Warning, 2, 12, "7.4.3"),
            // The standard prints this identifier with the group 62391; Q-022 records the
            // reading as a typo for 62351.
            Self::ShortKey => ("TLS_SHORT_KEY", Severity::Alarm, 2, 13, "7.4.3"),
            Self::DeprecatedHash => ("TLS_DEP_HASH", Severity::Warning, 2, 14, "7.4.3"),
        };
        Row {
            mnemonic,
            severity,
            group,
            number,
            clause,
        }
    }
}

/// Receives the security events that a connection raises. The application decides where they go:
/// a log, a monitor, or a test recorder.
pub trait SecurityEvents: Send + Sync {
    /// Records one event, with a short detail (for example the certificate subject).
    fn raise(&self, event: SecurityEvent, detail: &str);
}

/// A receiver that drops every event.
#[derive(Clone, Copy, Debug, Default)]
pub struct Discard;

impl SecurityEvents for Discard {
    fn raise(&self, _event: SecurityEvent, _detail: &str) {}
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn every_event_of_annex_a_is_listed_once() {
        assert_eq!(SecurityEvent::ALL.len(), 36);
        let mnemonics: HashSet<_> = SecurityEvent::ALL.iter().map(|e| e.mnemonic()).collect();
        assert_eq!(mnemonics.len(), 36, "mnemonics are unique");
    }

    #[test]
    fn identifiers_are_unique_except_the_printed_duplicate_of_2_8() {
        let identifiers: Vec<String> = SecurityEvent::ALL.iter().map(|e| e.identifier()).collect();
        let unique: HashSet<&String> = identifiers.iter().collect();
        assert_eq!(identifiers.len() - unique.len(), 1);
        let count = |id: &str| identifiers.iter().filter(|x| x.as_str() == id).count();
        assert_eq!(count("IEC 62351-3:2.8"), 2);
    }

    #[test]
    fn the_notice_and_the_alarms_follow_the_table() {
        assert_eq!(
            SecurityEvent::TlsHandshakeSuccess.severity(),
            Severity::Notice
        );
        assert_eq!(SecurityEvent::NoPeerCertificate.severity(), Severity::Alarm);
        assert_eq!(
            SecurityEvent::CertificateExpired.severity(),
            Severity::Alarm
        );
        assert_eq!(SecurityEvent::DeprecatedHash.severity(), Severity::Warning);
    }

    #[test]
    fn the_mnemonics_are_as_printed() {
        assert_eq!(
            SecurityEvent::CertificateRevoked.mnemonic(),
            "TLS_CERT_REVOKED"
        );
        assert_eq!(
            SecurityEvent::RevokedSessionClosed.mnemonic(),
            "TLS_SESSION_CLOSED-REV"
        );
        assert_eq!(SecurityEvent::ShortKey.identifier(), "IEC 62351-3:2.13");
    }
}
