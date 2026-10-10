// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! Bit-level access over byte slices.
//!
//! Bit numbering follows 5-4 4.3: bit 1 is the least significant bit of the
//! first octet, and consecutive bit numbering walks octets from their least
//! significant bit upward. The 104 transmits octets least-significant first
//! (104 9.5), so multi-octet numbers are read and written little-endian.
//!
//! The public entry points take a zero-based absolute bit index
//! (`bit number - 1`) and a width in bits.

/// Reads `width` bits (1..=64) starting at absolute bit index `start`,
/// assembled with bit `start` as the least significant bit of the result.
/// Returns `None` when the field would run past the end of `src`.
pub fn get_bits(src: &[u8], start: usize, width: usize) -> Option<u64> {
    if width == 0 || width > 64 {
        return None;
    }
    let last = start.checked_add(width.checked_sub(1)?)?;
    let needed = last.checked_div(8)?.checked_add(1)?;
    if needed > src.len() {
        return None;
    }
    let mut value: u64 = 0;
    for k in 0..width {
        let index = start.checked_add(k)?;
        let bit = (*src.get(index.checked_div(8)?)? >> index.checked_rem(8)?) & 1;
        if bit == 1 {
            value = value.checked_add(1u64.checked_shl(u32::try_from(k).ok()?)?)?;
        }
    }
    Some(value)
}

/// Writes `width` bits (1..=64) starting at absolute bit index `start` from
/// the low `width` bits of `value`, least significant bit of `value` going to
/// `start`. Returns `None` when the field would run past the end of `dst`;
/// the other bits of the touched octets are left untouched.
pub fn set_bits(dst: &mut [u8], start: usize, width: usize, value: u64) -> Option<()> {
    if width == 0 || width > 64 {
        return None;
    }
    let last = start.checked_add(width.checked_sub(1)?)?;
    let needed = last.checked_div(8)?.checked_add(1)?;
    if needed > dst.len() {
        return None;
    }
    for k in 0..width {
        let index = start.checked_add(k)?;
        let bit = (value >> k) & 1;
        let octet = dst.get_mut(index.checked_div(8)?)?;
        let mask = 1u8.checked_shl(u32::try_from(index.checked_rem(8)?).ok()?)?;
        if bit == 1 {
            *octet |= mask;
        } else {
            *octet &= !mask;
        }
    }
    Some(())
}

#[cfg(test)]
mod tests {
    use super::{get_bits, set_bits};

    #[test]
    fn within_one_octet() {
        // Bits 2..4 (1-based) of 0b0000_1100 are 0, 1, 1; bit 2 is the LSB of
        // the result, so the window is 0b110 = 6.
        assert_eq!(get_bits(&[0b0000_1100], 1, 3), Some(6));
    }

    #[test]
    fn spans_two_octets() {
        // Absolute bits 2..17 (16 wide): bit 1 is set in octet 1 and bit 17
        // is the LSB of octet 3, so the window reads 0b1000_0000_0000_0001.
        let src = [0x02, 0x00, 0x01];
        assert_eq!(get_bits(&src, 1, 16), Some(0x8001));
    }

    #[test]
    fn rejects_fields_running_past_the_buffer() {
        assert_eq!(get_bits(&[0xFF, 0xFF], 1, 17), None);
        assert_eq!(get_bits(&[], 0, 1), None);
        assert_eq!(get_bits(&[0x01], 0, 0), None);
        assert_eq!(get_bits(&[0x01], 0, 65), None);
    }

    #[test]
    fn set_spans_two_octets_and_keeps_other_bits() {
        let mut dst = [0b0000_0001u8, 0x00, 0x80];
        set_bits(&mut dst, 1, 16, 0x8001).expect("16 bits fit");
        // Field = indices 1..=16. Index 0 lies outside it and survives; index
        // 7 lies inside it and takes value bit 6 (0). Value bit 0 lands on
        // index 1 and value bit 15 on index 16. Index 23 survives.
        assert_eq!(dst, [0b0000_0011, 0x00, 0x81]);
    }

    #[test]
    fn set_clears_bits_it_owns() {
        let mut dst = [0xFFu8, 0xFF];
        set_bits(&mut dst, 0, 16, 0x0000).expect("16 bits fit");
        assert_eq!(dst, [0x00, 0x00]);
    }

    #[test]
    fn set_and_get_round_trip_cross_octet() {
        let mut dst = [0u8; 8];
        set_bits(&mut dst, 3, 20, 0xF_5A5).expect("20 bits fit");
        assert_eq!(get_bits(&dst, 3, 20), Some(0xF_5A5));
    }
}
