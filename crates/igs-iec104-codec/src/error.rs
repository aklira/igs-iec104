// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! Typed errors of the codec.

use std::fmt;

/// Why an ASDU or one of its fields could not be decoded.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum DecodeError {
    /// The input holds fewer octets than the field needs.
    Truncated {
        /// Octets the field needs.
        needed: usize,
        /// Octets the input holds.
        available: usize,
    },
    /// The type identification is not in the IEC 60870-5-101 catalogue.
    UnknownTypeId(u8),
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncated { needed, available } => {
                write!(f, "input truncated: needs {needed} octets, has {available}")
            }
            Self::UnknownTypeId(code) => write!(f, "unknown type identification {code}"),
        }
    }
}

impl std::error::Error for DecodeError {}

/// Why an ASDU or one of its fields could not be encoded.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum EncodeError {
    /// The destination holds fewer octets than the field needs.
    BufferTooSmall {
        /// Octets the field needs.
        needed: usize,
        /// Octets the destination holds.
        available: usize,
    },
}

impl fmt::Display for EncodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BufferTooSmall { needed, available } => {
                write!(
                    f,
                    "buffer too small: needs {needed} octets, has {available}"
                )
            }
        }
    }
}

impl std::error::Error for EncodeError {}
