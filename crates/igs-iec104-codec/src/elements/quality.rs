// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! Single-point and double-point information with quality (IEC 60870-5-101
//! 7.2.6.1 to 7.2.6.4 and 7.2.6.10).

use super::{begin, field_u8, flag, put, put_flag};

// Zero-based bit indexes of the quality bits, shared by every element below.
const BLOCKED: usize = 4;
const SUBSTITUTED: usize = 5;
const NOT_TOPICAL: usize = 6;
const INVALID: usize = 7;

/// The quality bits BL, SB, NT and IV (101 7.2.6.3).
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct QualityFlags {
    /// BL: the value is blocked for transmission.
    pub blocked: bool,
    /// SB: the value comes from an operator or an automatic source.
    pub substituted: bool,
    /// NT: the value was not updated successfully within the time interval.
    pub not_topical: bool,
    /// IV: the value is invalid and must not be used.
    pub invalid: bool,
}

impl QualityFlags {
    fn decode(src: &[u8]) -> Option<Self> {
        Some(Self {
            blocked: flag(src, BLOCKED)?,
            substituted: flag(src, SUBSTITUTED)?,
            not_topical: flag(src, NOT_TOPICAL)?,
            invalid: flag(src, INVALID)?,
        })
    }

    fn encode(self, dst: &mut [u8]) -> Option<()> {
        put_flag(dst, BLOCKED, self.blocked)?;
        put_flag(dst, SUBSTITUTED, self.substituted)?;
        put_flag(dst, NOT_TOPICAL, self.not_topical)?;
        put_flag(dst, INVALID, self.invalid)
    }
}

/// The two-bit code of a double-point value (101 7.2.6.2). Codes 0 and 3 both
/// mean "indeterminate", but they are kept apart so that every code round-trips.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum DoublePoint {
    /// Code 0: indeterminate or intermediate state.
    IndeterminateOrIntermediate,
    /// Code 1: determined state OFF.
    Off,
    /// Code 2: determined state ON.
    On,
    /// Code 3: indeterminate state.
    Indeterminate,
}

impl DoublePoint {
    fn from_code(code: u8) -> Option<Self> {
        match code {
            0 => Some(Self::IndeterminateOrIntermediate),
            1 => Some(Self::Off),
            2 => Some(Self::On),
            3 => Some(Self::Indeterminate),
            _ => None,
        }
    }

    const fn code(self) -> u64 {
        match self {
            Self::IndeterminateOrIntermediate => 0,
            Self::Off => 1,
            Self::On => 2,
            Self::Indeterminate => 3,
        }
    }
}

/// Quality descriptor QDS (101 7.2.6.3): overflow plus the quality bits.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct Qds {
    /// OV: the value is beyond its predefined range.
    pub overflow: bool,
    /// BL, SB, NT and IV.
    pub quality: QualityFlags,
}

impl Qds {
    /// Octets on the wire.
    pub const SIZE: usize = 1;

    /// Decodes the first octet of `src`.
    pub fn decode(src: &[u8]) -> Option<Self> {
        Some(Self {
            overflow: flag(src, 0)?,
            quality: QualityFlags::decode(src)?,
        })
    }

    /// Writes the octet into the front of `dst`.
    pub fn encode(self, dst: &mut [u8]) -> Option<()> {
        begin(dst, Self::SIZE)?;
        put_flag(dst, 0, self.overflow)?;
        self.quality.encode(dst)
    }
}

/// Single-point information with quality descriptor SIQ (101 7.2.6.1).
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct Siq {
    /// SPI: the point is ON (`true`) or OFF (`false`).
    pub on: bool,
    /// BL, SB, NT and IV.
    pub quality: QualityFlags,
}

impl Siq {
    /// Octets on the wire.
    pub const SIZE: usize = 1;

    /// Decodes the first octet of `src`.
    pub fn decode(src: &[u8]) -> Option<Self> {
        Some(Self {
            on: flag(src, 0)?,
            quality: QualityFlags::decode(src)?,
        })
    }

    /// Writes the octet into the front of `dst`.
    pub fn encode(self, dst: &mut [u8]) -> Option<()> {
        begin(dst, Self::SIZE)?;
        put_flag(dst, 0, self.on)?;
        self.quality.encode(dst)
    }
}

/// Double-point information with quality descriptor DIQ (101 7.2.6.2).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Diq {
    /// DPI: the two-bit state.
    pub state: DoublePoint,
    /// BL, SB, NT and IV.
    pub quality: QualityFlags,
}

impl Diq {
    /// Octets on the wire.
    pub const SIZE: usize = 1;

    /// Decodes the first octet of `src`.
    pub fn decode(src: &[u8]) -> Option<Self> {
        Some(Self {
            state: DoublePoint::from_code(field_u8(src, 0, 2)?)?,
            quality: QualityFlags::decode(src)?,
        })
    }

    /// Writes the octet into the front of `dst`.
    pub fn encode(self, dst: &mut [u8]) -> Option<()> {
        begin(dst, Self::SIZE)?;
        put(dst, 0, 2, self.state.code())?;
        self.quality.encode(dst)
    }
}

/// Quality descriptor of protection events QDP (101 7.2.6.4): elapsed time
/// invalid plus the quality bits.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct Qdp {
    /// EI: the elapsed time is invalid.
    pub elapsed_invalid: bool,
    /// BL, SB, NT and IV.
    pub quality: QualityFlags,
}

impl Qdp {
    /// Octets on the wire.
    pub const SIZE: usize = 1;

    /// Decodes the first octet of `src`.
    pub fn decode(src: &[u8]) -> Option<Self> {
        Some(Self {
            elapsed_invalid: flag(src, 3)?,
            quality: QualityFlags::decode(src)?,
        })
    }

    /// Writes the octet into the front of `dst`.
    pub fn encode(self, dst: &mut [u8]) -> Option<()> {
        begin(dst, Self::SIZE)?;
        put_flag(dst, 3, self.elapsed_invalid)?;
        self.quality.encode(dst)
    }
}

/// Single event of protection equipment SEP (101 7.2.6.10).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Sep {
    /// ES: the two-bit event state, coded like a double-point value.
    pub state: DoublePoint,
    /// EI: the elapsed time is invalid.
    pub elapsed_invalid: bool,
    /// BL, SB, NT and IV.
    pub quality: QualityFlags,
}

impl Sep {
    /// Octets on the wire.
    pub const SIZE: usize = 1;

    /// Decodes the first octet of `src`.
    pub fn decode(src: &[u8]) -> Option<Self> {
        Some(Self {
            state: DoublePoint::from_code(field_u8(src, 0, 2)?)?,
            elapsed_invalid: flag(src, 3)?,
            quality: QualityFlags::decode(src)?,
        })
    }

    /// Writes the octet into the front of `dst`.
    pub fn encode(self, dst: &mut [u8]) -> Option<()> {
        begin(dst, Self::SIZE)?;
        put(dst, 0, 2, self.state.code())?;
        put_flag(dst, 3, self.elapsed_invalid)?;
        self.quality.encode(dst)
    }
}
