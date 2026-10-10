// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! Typed errors of the codec.

use std::fmt;

/// Why an ASDU or one of its fields could not be decoded.
#[derive(Clone, Debug, PartialEq, Eq)]
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
    /// The type identification is in the catalogue but outside the 104
    /// profile. `raw` holds the whole ASDU as received.
    UnsupportedTypeId {
        /// The type identification code.
        type_code: u8,
        /// The ASDU octets, header included.
        raw: Vec<u8>,
    },
    /// A value of an information element is outside the range its clause allows.
    InvalidElement {
        /// Name of the element, as in IEC 60870-5-101 7.2.6.
        element: &'static str,
    },
    /// The ASDU is longer than the 249 octets the APDU length allows.
    TooLong {
        /// Octets received.
        length: usize,
        /// Largest ASDU length.
        max: usize,
    },
    /// The information objects end before the ASDU does.
    TrailingOctets {
        /// Octets left over.
        extra: usize,
    },
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncated { needed, available } => {
                write!(f, "input truncated: needs {needed} octets, has {available}")
            }
            Self::UnknownTypeId(code) => write!(f, "unknown type identification {code}"),
            Self::UnsupportedTypeId { type_code, .. } => {
                write!(
                    f,
                    "type identification {type_code} is not in the 104 profile"
                )
            }
            Self::InvalidElement { element } => write!(f, "invalid value of {element}"),
            Self::TooLong { length, max } => {
                write!(f, "ASDU of {length} octets exceeds the maximum of {max}")
            }
            Self::TrailingOctets { extra } => {
                write!(f, "{extra} octets follow the information objects")
            }
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
    /// More than 127 information objects (or element sets) do not fit in the
    /// variable structure qualifier.
    TooManyObjects {
        /// Objects requested.
        count: usize,
    },
    /// The ASDU is longer than the 249 octets the APDU length allows.
    TooLong {
        /// Octets the ASDU needs.
        length: usize,
        /// Largest ASDU length.
        max: usize,
    },
    /// A value does not fit in its field.
    ValueOutOfRange {
        /// Name of the field.
        field: &'static str,
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
            Self::TooManyObjects { count } => {
                write!(f, "{count} information objects exceed the maximum of 127")
            }
            Self::TooLong { length, max } => {
                write!(f, "ASDU of {length} octets exceeds the maximum of {max}")
            }
            Self::ValueOutOfRange { field } => write!(f, "{field} is out of range"),
        }
    }
}

impl std::error::Error for EncodeError {}
