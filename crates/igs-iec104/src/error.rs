// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! The errors of a connection.

use std::fmt;
use std::io;

use igs_iec104_codec::error::EncodeError;
use igs_iec104_link::CloseReason;

/// Why a connection could not be opened, or ended with an error.
#[derive(Debug)]
pub enum TransportError {
    /// The TCP connection was not established within t0 (§9.6).
    ConnectTimeout,
    /// The TCP connection could not be established.
    Connect(io::Error),
    /// The stream failed while the connection was running.
    Io(io::Error),
    /// The peer closed the stream.
    PeerClosed,
    /// The session closed the connection, for the reason given.
    Closed(CloseReason),
    /// A frame could not be encoded.
    Encode(EncodeError),
}

impl fmt::Display for TransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ConnectTimeout => write!(f, "the connection was not established within t0"),
            Self::Connect(error) => write!(f, "the connection could not be established: {error}"),
            Self::Io(error) => write!(f, "the stream failed: {error}"),
            Self::PeerClosed => write!(f, "the peer closed the connection"),
            Self::Closed(reason) => write!(f, "the session closed the connection ({reason:?})"),
            Self::Encode(error) => write!(f, "a frame could not be encoded: {error}"),
        }
    }
}

impl std::error::Error for TransportError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Connect(error) | Self::Io(error) => Some(error),
            Self::Encode(error) => Some(error),
            Self::ConnectTimeout | Self::PeerClosed | Self::Closed(_) => None,
        }
    }
}
