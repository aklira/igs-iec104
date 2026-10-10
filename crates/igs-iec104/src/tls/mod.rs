// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! TLS for IEC 60870-5-104, as IEC 62351-3 profiles it (task X1).
//!
//! [`profile`] holds the values that the standard's conformance tables mark as mandatory or
//! optional. [`events`] names the security events of its Annex A. [`settings`] collects the
//! identity, the trust anchors and the policy of a connection. A [`TlsBackend`] secures the TCP
//! connections of a client or a server: the rustls backend is the default feature `rustls`.

use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use tokio::net::TcpStream;

use crate::transport::BoxedTransport;

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
    /// The handshake failed: the peer refused the connection, or the certificates did not
    /// check out.
    Handshake(String),
}

impl fmt::Display for TlsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported(what) => write!(f, "not supported by this TLS backend: {what}"),
            Self::Invalid(why) => write!(f, "invalid TLS settings: {why}"),
            Self::Identity(why) => write!(f, "TLS identity: {why}"),
            Self::Handshake(why) => write!(f, "the TLS handshake failed: {why}"),
        }
    }
}

impl std::error::Error for TlsError {}

/// A handshake that a backend runs in the background: it yields the secured stream.
pub type Handshake<'a> =
    Pin<Box<dyn Future<Output = Result<BoxedTransport, TlsError>> + Send + 'a>>;

/// A TLS implementation of the profile. It secures the TCP connections of a client or a server,
/// and raises the security events of its handshakes. The trait is object-safe, so that the
/// configurations of the client and the server can hold any backend.
pub trait TlsBackend: Send + Sync {
    /// Secures an outgoing connection. The server certificate must name `host`, a DNS name or
    /// an IP address.
    fn connect<'a>(&'a self, tcp: TcpStream, host: &'a str) -> Handshake<'a>;

    /// Secures an accepted connection. The client certificate is required (clause 6.4.3).
    fn accept<'a>(&'a self, tcp: TcpStream) -> Handshake<'a>;
}

/// A TLS backend, shared by the configurations. Two configurations compare equal when they hold
/// the same backend.
#[derive(Clone)]
pub struct Secure(Arc<dyn TlsBackend>);

impl Secure {
    /// Wraps a backend for the configurations.
    pub fn new(backend: impl TlsBackend + 'static) -> Self {
        Self(Arc::new(backend))
    }

    /// The backend.
    pub fn backend(&self) -> &dyn TlsBackend {
        self.0.as_ref()
    }
}

impl fmt::Debug for Secure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secure(TLS backend)")
    }
}

impl PartialEq for Secure {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for Secure {}
