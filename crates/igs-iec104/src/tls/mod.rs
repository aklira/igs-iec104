// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! TLS for IEC 60870-5-104, as IEC 62351-3 profiles it (task X1).
//!
//! [`profile`] holds the values that the standard's conformance tables mark as mandatory or
//! optional. [`events`] names the security events of its Annex A. [`settings`] collects the
//! identity, the trust anchors and the policy of a connection. A backend turns the settings into
//! a secured stream: the rustls backend is the default feature `rustls`.

use std::fmt;

pub mod events;
pub mod profile;
pub mod settings;

#[cfg(feature = "rustls")]
pub mod rustls_backend;

/// A TLS setting, certificate or handshake that the profile or the backend refuses.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TlsError {
    /// The settings name something that the backend does not implement.
    Unsupported(String),
    /// The settings break a rule of the profile.
    Invalid(String),
    /// A certificate, key, trust anchor or revocation list could not be read or used.
    Identity(String),
}

impl fmt::Display for TlsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported(what) => write!(f, "not supported by this TLS backend: {what}"),
            Self::Invalid(why) => write!(f, "invalid TLS settings: {why}"),
            Self::Identity(why) => write!(f, "TLS identity: {why}"),
        }
    }
}

impl std::error::Error for TlsError {}
