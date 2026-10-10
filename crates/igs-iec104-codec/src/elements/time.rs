// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! Range times of the QueryLog request F_SC_NB_1 (IEC 60870-5-104 8.9).

use super::begin;
use crate::formats::Cp56Time2a;

/// One end of the range of a QueryLog request. The range times are
/// CP56Time2a (101 7.2.6.18), but all seven octets set to zero mean that the
/// range has no bound on this side. Such a value is not a valid CP56Time2a,
/// since its month and day are zero, so it cannot be confused with a time.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum RangeTime {
    /// All zeros: the range starts at the beginning or ends at the end of the log.
    Unbounded,
    /// A bounded time.
    At(Cp56Time2a),
}

impl RangeTime {
    /// Octets on the wire.
    pub const SIZE: usize = Cp56Time2a::SIZE;

    /// Decodes the first seven octets of `src`.
    pub fn decode(src: &[u8]) -> Option<Self> {
        if src.get(..Self::SIZE)?.iter().all(|&octet| octet == 0) {
            return Some(Self::Unbounded);
        }
        Some(Self::At(Cp56Time2a::decode(src)?))
    }

    /// Writes the seven octets into the front of `dst`.
    pub fn encode(self, dst: &mut [u8]) -> Option<()> {
        match self {
            Self::Unbounded => begin(dst, Self::SIZE),
            Self::At(time) => time.encode(dst),
        }
    }
}
