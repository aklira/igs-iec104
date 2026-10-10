// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! File transfer elements (IEC 60870-5-101 7.2.6.28 to 7.2.6.38 and 7.3.6.6).

use super::{begin, field, field_u8, flag, put, put_flag};

const MAX_NIBBLE: u8 = 15;
const MAX_FILE_READY: u8 = 127;
const MAX_LENGTH_OF_FILE: u32 = 0x00FF_FFFF;
const MAX_STATUS_OF_FILE: u8 = 31;

unsigned_element! {
    /// Name of file NOF (101 7.2.6.33). 0 is the default name.
    Nof, u16, size: 2, bits: 16
}

unsigned_element! {
    /// Name of section NOS (101 7.2.6.34). 0 is the default name.
    Nos, u8, size: 1, bits: 8
}

unsigned_element! {
    /// Length of segment LOS (101 7.2.6.36): the octets of the segment, 0 when
    /// not used.
    Los, u8, size: 1, bits: 8
}

unsigned_element! {
    /// Checksum CHS (101 7.2.6.37): the sum of the octets, modulo 256.
    Chs, u8, size: 1, bits: 8
}

unsigned_element! {
    /// Last section or segment qualifier LSQ (101 7.2.6.31).
    Lsq, u8, size: 1, bits: 8
}

impl Lsq {
    /// File transfer without deactivation.
    pub const FILE_WITHOUT_DEACTIVATION: Self = Self::new(1);
    /// File transfer with deactivation.
    pub const FILE_WITH_DEACTIVATION: Self = Self::new(2);
    /// Section transfer without deactivation.
    pub const SECTION_WITHOUT_DEACTIVATION: Self = Self::new(3);
    /// Section transfer with deactivation.
    pub const SECTION_WITH_DEACTIVATION: Self = Self::new(4);
}

/// Length of file or section LOF (101 7.2.6.35): 24 bits, 0 when not used.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Lof(u32);

impl Lof {
    /// Octets on the wire.
    pub const SIZE: usize = 3;

    /// Fails when `value` does not fit in 24 bits.
    pub const fn new(value: u32) -> Option<Self> {
        if value > MAX_LENGTH_OF_FILE {
            return None;
        }
        Some(Self(value))
    }

    /// The length in octets, 0..=16 777 215.
    pub const fn value(self) -> u32 {
        self.0
    }

    /// Decodes the first three octets of `src`.
    pub fn decode(src: &[u8]) -> Option<Self> {
        Self::new(u32::try_from(field(src, 0, 24)?).ok()?)
    }

    /// Writes the three octets into the front of `dst`.
    pub fn encode(self, dst: &mut [u8]) -> Option<()> {
        begin(dst, Self::SIZE)?;
        put(dst, 0, 24, u64::from(self.0))
    }
}

/// Status of file SOF (101 7.2.6.38).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Sof {
    status: u8,
    /// LFD: this is the last file of the directory.
    pub last_in_directory: bool,
    /// FOR: the name defines a subdirectory rather than a file.
    pub defines_subdirectory: bool,
    /// FA: the transfer of this file is active.
    pub transfer_active: bool,
}

impl Sof {
    /// Octets on the wire.
    pub const SIZE: usize = 1;

    /// Fails when `status` is above 31 (five bits). The flags start cleared.
    ///
    /// The standard gives the STATUS range as 0 to 32, but five bits cannot
    /// carry 32: see PROVENANCE.md D-009.
    pub const fn new(status: u8) -> Option<Self> {
        if status > MAX_STATUS_OF_FILE {
            return None;
        }
        Some(Self {
            status,
            last_in_directory: false,
            defines_subdirectory: false,
            transfer_active: false,
        })
    }

    /// The status code, 0..=31 (0 is the default).
    pub const fn status(self) -> u8 {
        self.status
    }

    /// Decodes the first octet of `src`.
    pub fn decode(src: &[u8]) -> Option<Self> {
        Some(Self {
            last_in_directory: flag(src, 5)?,
            defines_subdirectory: flag(src, 6)?,
            transfer_active: flag(src, 7)?,
            ..Self::new(field_u8(src, 0, 5)?)?
        })
    }

    /// Writes the octet into the front of `dst`.
    pub fn encode(self, dst: &mut [u8]) -> Option<()> {
        begin(dst, Self::SIZE)?;
        put(dst, 0, 5, u64::from(self.status))?;
        put_flag(dst, 5, self.last_in_directory)?;
        put_flag(dst, 6, self.defines_subdirectory)?;
        put_flag(dst, 7, self.transfer_active)
    }
}

/// File ready qualifier FRQ (101 7.2.6.28) and section ready qualifier SRQ
/// (101 7.2.6.29) share one layout: a seven-bit qualifier and a confirm bit.
macro_rules! ready_qualifier {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Copy, Clone, Debug, PartialEq, Eq)]
        pub struct $name {
            qualifier: u8,
            /// Set for a negative confirmation of the request.
            pub negative: bool,
        }

        impl $name {
            /// Octets on the wire.
            pub const SIZE: usize = 1;

            /// Fails when `qualifier` is above 127 (seven bits).
            pub const fn new(qualifier: u8, negative: bool) -> Option<Self> {
                if qualifier > MAX_FILE_READY {
                    return None;
                }
                Some(Self { qualifier, negative })
            }

            /// The qualifier, 0..=127 (0 is the default).
            pub const fn qualifier(self) -> u8 {
                self.qualifier
            }

            /// Decodes the first octet of `src`.
            pub fn decode(src: &[u8]) -> Option<Self> {
                Self::new(field_u8(src, 0, 7)?, flag(src, 7)?)
            }

            /// Writes the octet into the front of `dst`.
            pub fn encode(self, dst: &mut [u8]) -> Option<()> {
                begin(dst, Self::SIZE)?;
                put(dst, 0, 7, u64::from(self.qualifier))?;
                put_flag(dst, 7, self.negative)
            }
        }
    };
}

ready_qualifier! {
    /// File ready qualifier FRQ (101 7.2.6.28).
    Frq
}

ready_qualifier! {
    /// Section ready qualifier SRQ (101 7.2.6.29).
    Srq
}

/// Pair of four-bit codes: the low nibble is bits 1 to 4, the high nibble
/// bits 5 to 8 of the octet.
macro_rules! nibble_pair {
    ($(#[$doc:meta])* $name:ident, $first:ident, $second:ident) => {
        $(#[$doc])*
        #[derive(Copy, Clone, Debug, PartialEq, Eq)]
        pub struct $name {
            $first: u8,
            $second: u8,
        }

        impl $name {
            /// Octets on the wire.
            pub const SIZE: usize = 1;

            /// Fails when either code is above 15 (four bits).
            pub const fn new($first: u8, $second: u8) -> Option<Self> {
                if $first > MAX_NIBBLE || $second > MAX_NIBBLE {
                    return None;
                }
                Some(Self { $first, $second })
            }

            /// The low nibble, 0..=15.
            pub const fn $first(self) -> u8 {
                self.$first
            }

            /// The high nibble, 0..=15.
            pub const fn $second(self) -> u8 {
                self.$second
            }

            /// Decodes the first octet of `src`.
            pub fn decode(src: &[u8]) -> Option<Self> {
                Self::new(field_u8(src, 0, 4)?, field_u8(src, 4, 4)?)
            }

            /// Writes the octet into the front of `dst`.
            pub fn encode(self, dst: &mut [u8]) -> Option<()> {
                begin(dst, Self::SIZE)?;
                put(dst, 0, 4, u64::from(self.$first))?;
                put(dst, 4, 4, u64::from(self.$second))
            }
        }
    };
}

nibble_pair! {
    /// Select and call qualifier SCQ (101 7.2.6.30): the request (low nibble)
    /// and the error code (high nibble).
    Scq, command, error
}

nibble_pair! {
    /// Acknowledge file or section qualifier AFQ (101 7.2.6.32): the
    /// acknowledgement (low nibble) and the error code (high nibble).
    Afq, acknowledge, error
}

/// The data of a file segment F_SG_NA_1 (101 7.3.6.6). Its length is the
/// length of segment element (LOS) of the same ASDU, so the type has no fixed
/// `SIZE`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Segment {
    data: Vec<u8>,
}

impl Segment {
    /// Reads `length` octets from the front of `src`. Fails when `src` is
    /// shorter than `length`.
    pub fn decode(src: &[u8], length: usize) -> Option<Self> {
        Some(Self {
            data: src.get(..length)?.to_vec(),
        })
    }

    /// Writes the segment into the front of `dst`. Fails when `dst` cannot hold it.
    pub fn encode(&self, dst: &mut [u8]) -> Option<()> {
        dst.get_mut(..self.data.len())?.copy_from_slice(&self.data);
        Some(())
    }

    /// The octets of the segment.
    pub fn data(&self) -> &[u8] {
        &self.data
    }
}
