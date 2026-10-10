// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! Normalised fixed-point number F16 (5-4 6.4.1) and IEEE 754 short real
//! R32 (5-4 6.5), both transmitted least-significant octet first.

use super::bits::{get_bits, set_bits};

/// F16: a signed normalised value on 16 bits, in the range
/// -1..=+1 - 2^-15. The internal integer is the two's-complement value of
/// the 16-bit field; the numeric value is that integer divided by 2^15.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct F16 {
    raw: i16,
}

/// The scaling factor of F16: 2^15.
const F16_SCALE: f32 = 32768.0;

impl F16 {
    /// Builds F16 from the raw 16-bit field value. Every `i16` is a valid
    /// F16 encoding (5-4 6.4.1).
    pub const fn from_raw(raw: i16) -> Self {
        Self { raw }
    }

    /// Builds F16 from a float. Returns `None` when the value does not fit
    /// the range, is not finite, or is not exactly representable on the F16
    /// grid (a multiple of 2^-15).
    pub fn try_from_f32(value: f32) -> Option<Self> {
        if !value.is_finite() {
            return None;
        }
        if !(-1.0..1.0).contains(&value) {
            return None;
        }
        let scaled = value * F16_SCALE;
        if scaled < -32768.0 || scaled > 32768.0 {
            return None;
        }
        let rounded = scaled.round_ties_even();
        if rounded != scaled {
            return None;
        }
        if rounded == 32768.0 {
            return None;
        }
        let raw = i16::try_from(rounded as i32).ok()?;
        Some(Self::from_raw(raw))
    }

    /// The numeric value of this F16.
    pub fn as_f32(self) -> f32 {
        f32::from(self.raw) / F16_SCALE
    }

    /// Decodes the first two little-endian octets of `src` (5-4 6.4.1).
    pub fn decode(src: &[u8]) -> Option<Self> {
        let bits = u16::try_from(get_bits(src, 0, 16)?).ok()?;
        Some(Self::from_raw(bits as i16))
    }

    /// Writes the two little-endian octets into the front of `dst`.
    pub fn encode(self, dst: &mut [u8]) -> Option<()> {
        set_bits(dst, 0, 16, u64::from(self.raw as u16))
    }

    /// The raw 16-bit field value.
    pub const fn raw(self) -> i16 {
        self.raw
    }
}

/// R32: a real number in IEEE 754 single precision (5-4 6.5), using
/// `f32::from_bits` / `to_bits`. NaN survives a round trip by bit pattern,
/// but the IEEE rule "NaN != NaN" makes `PartialEq` false for NaN values.
#[derive(Copy, Clone, Debug)]
pub struct R32 {
    bits: u32,
}

impl R32 {
    /// Wraps the 32 IEEE 754 bit pattern.
    pub const fn from_bits(bits: u32) -> Self {
        Self { bits }
    }

    /// Builds R32 from a float value (IEEE 754 round to nearest even).
    pub fn from_f32(value: f32) -> Self {
        Self::from_bits(value.to_bits())
    }

    /// The numeric value.
    pub fn as_f32(self) -> f32 {
        f32::from_bits(self.bits)
    }

    /// Decodes the first four little-endian octets of `src` (5-4 6.5).
    pub fn decode(src: &[u8]) -> Option<Self> {
        let bits = u32::try_from(get_bits(src, 0, 32)?).ok()?;
        Some(Self::from_bits(bits))
    }

    /// Writes the four little-endian octets into the front of `dst`.
    pub fn encode(self, dst: &mut [u8]) -> Option<()> {
        set_bits(dst, 0, 32, u64::from(self.bits))
    }

    /// The IEEE 754 bit pattern.
    pub const fn to_bits(self) -> u32 {
        self.bits
    }
}

impl PartialEq for R32 {
    /// Equality of bit patterns, so a NaN value compares equal to the same
    /// NaN bit pattern.
    fn eq(&self, other: &Self) -> bool {
        self.bits == other.bits
    }
}

impl Eq for R32 {}

#[cfg(test)]
mod tests {
    use super::{F16, F16_SCALE, R32};

    #[test]
    fn f16_boundaries_hand_written_vectors() {
        let mut dst = [0x00u8; 2];

        // -1 = raw -32768 = 0x8000, little-endian octets 0x00, 0x80.
        F16::from_raw(-32768).encode(&mut dst).expect("two octets");
        assert_eq!(dst, [0x00, 0x80]);
        assert_eq!(F16::from_raw(-32768).as_f32(), -1.0);

        // +1 - 2^-15 = raw 32767 = 0x7FFF, little-endian 0xFF, 0x7F.
        F16::from_raw(32767).encode(&mut dst).expect("two octets");
        assert_eq!(dst, [0xFF, 0x7F]);
        assert_eq!(F16::from_raw(32767).as_f32(), 1.0 - 1.0 / F16_SCALE);

        // +0.5 = raw 16384 = 0x4000, little-endian 0x00, 0x40.
        F16::from_raw(16384).encode(&mut dst).expect("two octets");
        assert_eq!(dst, [0x00, 0x40]);
        assert_eq!(F16::from_raw(16384).as_f32(), 0.5);
    }

    #[test]
    fn f16_from_f32_range_checks() {
        assert_eq!(F16::try_from_f32(1.0), None);
        assert_eq!(F16::try_from_f32(1.5), None);
        assert_eq!(F16::try_from_f32(f32::NAN), None);
        assert_eq!(F16::try_from_f32(f32::INFINITY), None);
        // 0.1 is not a multiple of 2^-15.
        assert_eq!(F16::try_from_f32(0.1), None);
        assert_eq!(F16::try_from_f32(-1.0).map(F16::as_f32), Some(-1.0));
        assert_eq!(F16::try_from_f32(0.5).map(F16::as_f32), Some(0.5));
        assert_eq!(
            F16::try_from_f32(1.0 - 1.0 / 32768.0).map(F16::raw),
            Some(32767)
        );
    }

    #[test]
    fn f16_round_trip() {
        for raw in [-32768i16, -1, 0, 1, 7, 32767] {
            let mut dst = [0x00u8; 2];
            let value = F16::from_raw(raw);
            value.encode(&mut dst).expect("two octets");
            assert_eq!(F16::decode(&dst), Some(value));
        }
    }

    #[test]
    fn r32_hand_written_vectors() {
        // IEEE 754 single: 1.0 = 0x3F800000 sent least-significant octet
        // first as 00 00 80 3F.
        assert_eq!(R32::from_f32(1.0).to_bits(), 0x3F80_0000);
        let mut dst = [0x00u8; 4];
        R32::from_f32(1.0).encode(&mut dst).expect("four octets");
        assert_eq!(dst, [0x00, 0x00, 0x80, 0x3F]);

        // -1.0 = 0xBF800000 -> 00 00 80 BF.
        R32::from_f32(-1.0).encode(&mut dst).expect("four octets");
        assert_eq!(dst, [0x00, 0x00, 0x80, 0xBF]);
    }

    #[test]
    fn r32_round_trip_including_specials() {
        for value in [0.0f32, -0.0, 1.5, -1.5, f32::MAX, f32::MIN_POSITIVE] {
            let mut dst = [0x00u8; 4];
            let real = R32::from_f32(value);
            real.encode(&mut dst).expect("four octets");
            assert_eq!(R32::decode(&dst), Some(real));
            assert_eq!(R32::decode(&dst).map(R32::as_f32), Some(value));
        }
        let inf = R32::from_bits(0x7F80_0000);
        assert_eq!(inf.as_f32(), f32::INFINITY);
        assert!(R32::from_bits(0x7FC0_0000).as_f32().is_nan());
    }

    #[test]
    fn truncated_input_rejected() {
        assert_eq!(F16::decode(&[]), None);
        assert_eq!(F16::decode(&[0x00]), None);
        assert_eq!(R32::decode(&[]), None);
        assert_eq!(R32::decode(&[0x00, 0x00, 0x80]), None);
    }
}
