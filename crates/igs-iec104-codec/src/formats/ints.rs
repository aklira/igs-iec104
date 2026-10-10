// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! Signed integer I16 (5-4 5.2.1): a 16-bit two's-complement value in the
//! range -32768..=32767, transmitted least-significant octet first.

use super::bits::{get_bits, set_bits};

/// A 16-bit signed integer.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct I16 {
    value: i16,
}

impl I16 {
    /// Octets on the wire.
    pub const SIZE: usize = 2;

    /// Wraps a value; every `i16` is in range (5-4 5.2.1).
    pub const fn new(value: i16) -> Self {
        Self { value }
    }

    /// Decodes the first two little-endian octets of `src` into a signed
    /// value. Fails when `src` is shorter than two octets.
    pub fn decode(src: &[u8]) -> Option<Self> {
        let bits = u16::try_from(get_bits(src, 0, 16)?).ok()?;
        Some(Self::new(bits as i16))
    }

    /// Writes the two little-endian octets into the front of `dst`; returns
    /// `None` when `dst` cannot hold two octets.
    pub fn encode(self, dst: &mut [u8]) -> Option<()> {
        set_bits(dst, 0, 16, u64::from(self.value as u16))
    }

    /// The decoded value.
    pub const fn value(self) -> i16 {
        self.value
    }
}

#[cfg(test)]
mod tests {
    use super::I16;

    #[test]
    fn boundaries_hand_written_vectors() {
        let mut dst = [0x00u8; 2];

        I16::new(-1).encode(&mut dst).expect("two octets");
        assert_eq!(dst, [0xFF, 0xFF]);
        I16::new(-32768).encode(&mut dst).expect("two octets");
        assert_eq!(dst, [0x00, 0x80]);
        I16::new(32767).encode(&mut dst).expect("two octets");
        assert_eq!(dst, [0xFF, 0x7F]);
    }

    #[test]
    fn decode_hand_written_vectors() {
        assert_eq!(I16::decode(&[0xFF, 0xFF]).map(I16::value), Some(-1));
        assert_eq!(I16::decode(&[0x00, 0x80]).map(I16::value), Some(-32768));
        assert_eq!(I16::decode(&[0xFF, 0x7F]).map(I16::value), Some(32767));
    }

    #[test]
    fn round_trip() {
        for value in [-32768i16, -256, -1, 0, 1, 1234, 32767] {
            let mut dst = [0x00u8; 2];
            I16::new(value).encode(&mut dst).expect("two octets");
            assert_eq!(I16::decode(&dst).map(I16::value), Some(value));
        }
    }

    #[test]
    fn rejects_truncated_input() {
        assert_eq!(I16::decode(&[0xFF]), None);
        assert_eq!(I16::decode(&[]), None);
    }
}
