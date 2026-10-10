// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! Bit string BS(n) (5-4 5.6): a register of n independent state bits.

use super::bits::{get_bits, set_bits};

/// A bit string of 1..=64 bits. Bit `i` of the public accessors and of the
/// encoding is the i-th bit counting from the LSB of the first octet.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Bs {
    bits: u64,
    width: u8,
}

impl Bs {
    /// Builds a bit string from a raw value. Fails when the width is out of
    /// the 1..=64 range or the value does not fit in `width` bits.
    pub fn new(width: usize, value: u64) -> Option<Self> {
        if width == 0 || width > 64 {
            return None;
        }
        if width < 64 && value >= 1u64.checked_shl(u32::try_from(width).ok()?)? {
            return None;
        }
        let width = u8::try_from(width).ok()?;
        Some(Self { bits: value, width })
    }

    /// Decodes `width` bits from the start of `src` (5-4 5.6). Fails when
    /// `src` is shorter than the width requires or the width is out of
    /// range.
    pub fn decode(src: &[u8], width: usize) -> Option<Self> {
        if width == 0 || width > 64 {
            return None;
        }
        let bits = get_bits(src, 0, width)?;
        Self::new(width, bits)
    }

    /// Writes the bit string into `dst`, which must hold at least `width`
    /// bits. Bits of `dst` beyond the width keep their value.
    pub fn encode(self, dst: &mut [u8]) -> Option<()> {
        let width = usize::from(self.width);
        set_bits(dst, 0, width, self.bits)
    }

    /// The state bit at index `index`, counted from 0 at the LSB of the
    /// first octet. Returns `None` past the width.
    pub fn bit(self, index: usize) -> Option<bool> {
        if index >= usize::from(self.width) {
            return None;
        }
        Some(((self.bits >> index) & 1) == 1)
    }

    /// The number of bits of this bit string.
    pub fn width(self) -> usize {
        usize::from(self.width)
    }
}

#[cfg(test)]
mod tests {
    use super::Bs;

    #[test]
    fn new_rejects_bad_width_or_value() {
        assert!(Bs::new(0, 0).is_none());
        assert!(Bs::new(65, 0).is_none());
        assert!(Bs::new(64, u64::MAX).is_some());
        assert!(Bs::new(4, 16).is_none());
        assert!(Bs::new(4, 15).is_some());
    }

    #[test]
    fn bit_access_follows_bit_numbering() {
        let bs = Bs::new(6, 0b010_110).expect("5 bits fit in 6");
        assert_eq!(bs.bit(0), Some(false));
        assert_eq!(bs.bit(1), Some(true));
        assert_eq!(bs.bit(5), Some(false));
        assert_eq!(bs.bit(6), None);
        assert_eq!(bs.width(), 6);
    }

    #[test]
    fn hand_written_vector() {
        // Octet 0x34 = 0b0011_0100: the six state bits, counted from the LSB,
        // are 0, 0, 1, 0, 1, 1. The bits above the width (6 and 7) are ignored.
        let bs = Bs::decode(&[0x34, 0xFF], 6).expect("6 bits fit in one octet");
        assert_eq!(bs.bit(0), Some(false));
        assert_eq!(bs.bit(1), Some(false));
        assert_eq!(bs.bit(2), Some(true));
        assert_eq!(bs.bit(3), Some(false));
        assert_eq!(bs.bit(4), Some(true));
        assert_eq!(bs.bit(5), Some(true));
        let mut dst = [0x00u8; 2];
        bs.encode(&mut dst).expect("encode fits");
        assert_eq!(dst, [0x34, 0x00]);
    }

    #[test]
    fn rejects_truncated_input() {
        assert!(Bs::decode(&[], 1).is_none());
        assert!(Bs::decode(&[0x00], 9).is_none());
    }

    #[test]
    fn round_trip_48_bits() {
        let src = [0xDEu8, 0xAD, 0xBE, 0xEF, 0x00, 0x10];
        let bs = Bs::decode(&src, 48).expect("48 bits fit");
        let mut dst = [0x00u8; 6];
        bs.encode(&mut dst).expect("48 bits fit");
        assert_eq!(dst, src);
    }
}
