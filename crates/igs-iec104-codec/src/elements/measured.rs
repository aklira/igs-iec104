// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! Measured values that are not plain numbers (IEC 60870-5-101 7.2.6.5).

use super::{begin, field, flag, put, put_flag};

const MIN_VALUE: i8 = -64;
const MAX_VALUE: i8 = 63;

/// Value with transient state indication VTI (101 7.2.6.5): a seven-bit
/// two's-complement step position and the transient flag.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Vti {
    value: i8,
    transient: bool,
}

impl Vti {
    /// Octets on the wire.
    pub const SIZE: usize = 1;

    /// Fails when `value` is outside -64..=63.
    pub const fn new(value: i8, transient: bool) -> Option<Self> {
        if value < MIN_VALUE || value > MAX_VALUE {
            return None;
        }
        Some(Self { value, transient })
    }

    /// Decodes the first octet of `src`.
    pub fn decode(src: &[u8]) -> Option<Self> {
        let raw = u8::try_from(field(src, 0, 7)?).ok()?;
        // Sign-extend the seven-bit value: bit 6 is the sign.
        let value = if raw & 0x40 != 0 {
            (raw | 0x80) as i8
        } else {
            raw as i8
        };
        Self::new(value, flag(src, 7)?)
    }

    /// Writes the octet into the front of `dst`.
    pub fn encode(self, dst: &mut [u8]) -> Option<()> {
        begin(dst, Self::SIZE)?;
        put(dst, 0, 7, u64::from((self.value as u8) & 0x7F))?;
        put_flag(dst, 7, self.transient)
    }

    /// The step position, -64..=63.
    pub const fn value(self) -> i8 {
        self.value
    }

    /// True when the equipment is in a transient state.
    pub const fn is_transient(self) -> bool {
        self.transient
    }
}
