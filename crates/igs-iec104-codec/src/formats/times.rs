// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! Time tags CP56Time2a, CP24Time2a and CP16Time2a (5-4 6.8, 101 7.2.6.18 to
//! 7.2.6.20). Bit positions and ranges come from the data file `formats.yaml`;
//! octets are little-endian (104 9.5).
//!
//! Constructors check every range and decoders go through the same checks,
//! so a time tag that exists always holds valid values. Reserved bits are
//! written as 0 and ignored when decoding.

use super::bits::{get_bits, set_bits};

const MAX_MILLISECONDS: u16 = 59_999;
const MAX_MINUTES: u8 = 59;
const MAX_HOURS: u8 = 23;
const MAX_DAY_OF_MONTH: u8 = 31;
const MAX_DAY_OF_WEEK: u8 = 7;
const MAX_MONTH: u8 = 12;
const MAX_YEAR: u8 = 99;

// (zero-based first bit, width). formats.yaml numbers bits from 1, so bit n
// there is index n - 1 here.
const MILLISECONDS: (usize, usize) = (0, 16); // bits 1..16
const MINUTES: (usize, usize) = (16, 6); // bits 17..22
const RES1: (usize, usize) = (22, 1); // bit 23, GEN in 101
const INVALID: (usize, usize) = (23, 1); // bit 24, IV
const HOURS: (usize, usize) = (24, 5); // bits 25..29
const RES2: (usize, usize) = (29, 2); // bits 30..31
const SUMMER_TIME: (usize, usize) = (31, 1); // bit 32, SU
const DAY_OF_MONTH: (usize, usize) = (32, 5); // bits 33..37
const DAY_OF_WEEK: (usize, usize) = (37, 3); // bits 38..40
const MONTH: (usize, usize) = (40, 4); // bits 41..44
const RES3: (usize, usize) = (44, 4); // bits 45..48
const YEARS: (usize, usize) = (48, 7); // bits 49..55
const RES4: (usize, usize) = (55, 1); // bit 56

/// Octets of a CP56Time2a.
const CP56_OCTETS: usize = 7;
/// Octets of a CP24Time2a.
const CP24_OCTETS: usize = 3;
/// Octets of a CP16Time2a.
const CP16_OCTETS: usize = 2;

fn read(src: &[u8], (start, width): (usize, usize)) -> Option<u64> {
    get_bits(src, start, width)
}

fn read_u8(src: &[u8], field: (usize, usize)) -> Option<u8> {
    u8::try_from(read(src, field)?).ok()
}

fn read_flag(src: &[u8], field: (usize, usize)) -> Option<bool> {
    Some(read(src, field)? == 1)
}

fn write(dst: &mut [u8], (start, width): (usize, usize), value: u64) -> Option<()> {
    set_bits(dst, start, width, value)
}

/// CP16Time2a (101 7.2.6.20): milliseconds within the minute, 0..=59 999.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Cp16Time2a {
    milliseconds: u16,
}

impl Cp16Time2a {
    /// Octets on the wire.
    pub const SIZE: usize = CP16_OCTETS;

    /// Fails when `milliseconds` is above 59 999.
    pub const fn new(milliseconds: u16) -> Option<Self> {
        if milliseconds > MAX_MILLISECONDS {
            return None;
        }
        Some(Self { milliseconds })
    }

    /// Decodes the first two octets of `src`. Fails on truncated input or
    /// an out-of-range value.
    pub fn decode(src: &[u8]) -> Option<Self> {
        let milliseconds = u16::try_from(read(src, MILLISECONDS)?).ok()?;
        Self::new(milliseconds)
    }

    /// Writes the two octets into the front of `dst`.
    pub fn encode(self, dst: &mut [u8]) -> Option<()> {
        if dst.len() < CP16_OCTETS {
            return None;
        }
        write(dst, MILLISECONDS, u64::from(self.milliseconds))
    }

    /// The milliseconds within the minute, 0 to 59 999.
    pub const fn milliseconds(self) -> u16 {
        self.milliseconds
    }
}

/// CP24Time2a (101 7.2.6.19): milliseconds and minutes, plus the IV and RES1
/// flags. Forbidden in the 104 profile, kept for the 101 link layer.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Cp24Time2a {
    milliseconds: u16,
    minutes: u8,
    invalid: bool,
    substituted: bool,
}

impl Cp24Time2a {
    /// Octets on the wire.
    pub const SIZE: usize = CP24_OCTETS;

    /// Fails when `milliseconds` is above 59 999 or `minutes` above 59.
    pub const fn new(milliseconds: u16, minutes: u8) -> Option<Self> {
        if milliseconds > MAX_MILLISECONDS || minutes > MAX_MINUTES {
            return None;
        }
        Some(Self {
            milliseconds,
            minutes,
            invalid: false,
            substituted: false,
        })
    }

    /// Sets the IV flag (the time is invalid).
    pub const fn with_invalid(self, invalid: bool) -> Self {
        Self { invalid, ..self }
    }

    /// Sets RES1, which 101 names GEN: the time is substituted, not real.
    pub const fn with_substituted(self, substituted: bool) -> Self {
        Self {
            substituted,
            ..self
        }
    }

    /// Decodes the first three octets of `src`.
    pub fn decode(src: &[u8]) -> Option<Self> {
        if src.len() < CP24_OCTETS {
            return None;
        }
        let milliseconds = u16::try_from(read(src, MILLISECONDS)?).ok()?;
        let minutes = read_u8(src, MINUTES)?;
        Some(
            Self::new(milliseconds, minutes)?
                .with_substituted(read_flag(src, RES1)?)
                .with_invalid(read_flag(src, INVALID)?),
        )
    }

    /// Writes the three octets into the front of `dst`.
    pub fn encode(self, dst: &mut [u8]) -> Option<()> {
        if dst.len() < CP24_OCTETS {
            return None;
        }
        write(dst, MILLISECONDS, u64::from(self.milliseconds))?;
        write(dst, MINUTES, u64::from(self.minutes))?;
        write(dst, RES1, u64::from(self.substituted))?;
        write(dst, INVALID, u64::from(self.invalid))
    }

    /// The milliseconds within the minute, 0 to 59 999.
    pub const fn milliseconds(self) -> u16 {
        self.milliseconds
    }

    /// The minutes, 0 to 59.
    pub const fn minutes(self) -> u8 {
        self.minutes
    }

    /// The IV flag: the time is invalid.
    pub const fn is_invalid(self) -> bool {
        self.invalid
    }

    /// The RES1 bit, which 101 names GEN: the time is substituted, not real.
    pub const fn is_substituted(self) -> bool {
        self.substituted
    }
}

/// CP56Time2a (5-4 6.8): a full date and time, plus the IV, SU and RES1
/// flags. The day of the week is 0 when not used, otherwise 1 (Monday) to 7.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Cp56Time2a {
    milliseconds: u16,
    minutes: u8,
    hours: u8,
    day_of_month: u8,
    day_of_week: u8,
    month: u8,
    year: u8,
    invalid: bool,
    summer_time: bool,
    substituted: bool,
}

impl Cp56Time2a {
    /// Octets on the wire.
    pub const SIZE: usize = CP56_OCTETS;

    /// Fails when a field is outside its range: milliseconds 0..=59 999,
    /// minutes 0..=59, hours 0..=23, day of month 1..=31, day of week
    /// 0..=7, month 1..=12, year 0..=99.
    pub const fn new(
        milliseconds: u16,
        minutes: u8,
        hours: u8,
        day_of_month: u8,
        day_of_week: u8,
        month: u8,
        year: u8,
    ) -> Option<Self> {
        if milliseconds > MAX_MILLISECONDS
            || minutes > MAX_MINUTES
            || hours > MAX_HOURS
            || day_of_month == 0
            || day_of_month > MAX_DAY_OF_MONTH
            || day_of_week > MAX_DAY_OF_WEEK
            || month == 0
            || month > MAX_MONTH
            || year > MAX_YEAR
        {
            return None;
        }
        Some(Self {
            milliseconds,
            minutes,
            hours,
            day_of_month,
            day_of_week,
            month,
            year,
            invalid: false,
            summer_time: false,
            substituted: false,
        })
    }

    /// Sets the IV flag (the time is invalid).
    pub const fn with_invalid(self, invalid: bool) -> Self {
        Self { invalid, ..self }
    }

    /// Sets the SU flag (summer time).
    pub const fn with_summer_time(self, summer_time: bool) -> Self {
        Self {
            summer_time,
            ..self
        }
    }

    /// Sets RES1, which 101 names GEN: the time is substituted, not real.
    pub const fn with_substituted(self, substituted: bool) -> Self {
        Self {
            substituted,
            ..self
        }
    }

    /// Decodes the first seven octets of `src`.
    pub fn decode(src: &[u8]) -> Option<Self> {
        if src.len() < CP56_OCTETS {
            return None;
        }
        let milliseconds = u16::try_from(read(src, MILLISECONDS)?).ok()?;
        Some(
            Self::new(
                milliseconds,
                read_u8(src, MINUTES)?,
                read_u8(src, HOURS)?,
                read_u8(src, DAY_OF_MONTH)?,
                read_u8(src, DAY_OF_WEEK)?,
                read_u8(src, MONTH)?,
                read_u8(src, YEARS)?,
            )?
            .with_substituted(read_flag(src, RES1)?)
            .with_invalid(read_flag(src, INVALID)?)
            .with_summer_time(read_flag(src, SUMMER_TIME)?),
        )
    }

    /// Writes the seven octets into the front of `dst`.
    pub fn encode(self, dst: &mut [u8]) -> Option<()> {
        if dst.len() < CP56_OCTETS {
            return None;
        }
        write(dst, MILLISECONDS, u64::from(self.milliseconds))?;
        write(dst, MINUTES, u64::from(self.minutes))?;
        write(dst, RES1, u64::from(self.substituted))?;
        write(dst, INVALID, u64::from(self.invalid))?;
        write(dst, HOURS, u64::from(self.hours))?;
        write(dst, RES2, 0)?;
        write(dst, SUMMER_TIME, u64::from(self.summer_time))?;
        write(dst, DAY_OF_MONTH, u64::from(self.day_of_month))?;
        write(dst, DAY_OF_WEEK, u64::from(self.day_of_week))?;
        write(dst, MONTH, u64::from(self.month))?;
        write(dst, RES3, 0)?;
        write(dst, YEARS, u64::from(self.year))?;
        write(dst, RES4, 0)
    }

    /// The milliseconds within the minute, 0 to 59 999.
    pub const fn milliseconds(self) -> u16 {
        self.milliseconds
    }

    /// The minutes, 0 to 59.
    pub const fn minutes(self) -> u8 {
        self.minutes
    }

    /// The hours, 0 to 23.
    pub const fn hours(self) -> u8 {
        self.hours
    }

    /// The day of the month, 1 to 31.
    pub const fn day_of_month(self) -> u8 {
        self.day_of_month
    }

    /// The day of the week: 0 when not used, otherwise 1 (Monday) to 7.
    pub const fn day_of_week(self) -> u8 {
        self.day_of_week
    }

    /// The month, 1 to 12.
    pub const fn month(self) -> u8 {
        self.month
    }

    /// The year within the century, 0 to 99.
    pub const fn year(self) -> u8 {
        self.year
    }

    /// The IV flag: the time is invalid.
    pub const fn is_invalid(self) -> bool {
        self.invalid
    }

    /// The SU flag: summer time is in effect.
    pub const fn is_summer_time(self) -> bool {
        self.summer_time
    }

    /// The RES1 bit, which 101 names GEN: the time is substituted, not real.
    pub const fn is_substituted(self) -> bool {
        self.substituted
    }
}

#[cfg(test)]
mod tests {
    use super::{Cp16Time2a, Cp24Time2a, Cp56Time2a};

    fn cp56_min() -> Cp56Time2a {
        Cp56Time2a::new(0, 0, 0, 1, 0, 1, 0).expect("minimum values are in range")
    }

    fn cp56_max() -> Cp56Time2a {
        Cp56Time2a::new(59_999, 59, 23, 31, 7, 12, 99)
            .expect("maximum values are in range")
            .with_invalid(true)
            .with_summer_time(true)
            .with_substituted(true)
    }

    #[test]
    fn cp56_hand_written_vectors() {
        // 1000 ms, 30 min, 12 h, 15th, Wednesday (3), July, 2024 (year 24),
        // summer time. Each octet is derived from the bit positions of
        // formats.yaml.
        let example = Cp56Time2a::new(1000, 30, 12, 15, 3, 7, 24)
            .expect("in range")
            .with_summer_time(true);
        let mut dst = [0u8; 7];
        example.encode(&mut dst).expect("seven octets");
        assert_eq!(dst, [0xE8, 0x03, 0x1E, 0x8C, 0x6F, 0x07, 0x18]);

        cp56_min().encode(&mut dst).expect("seven octets");
        assert_eq!(dst, [0x00, 0x00, 0x00, 0x00, 0x01, 0x01, 0x00]);

        cp56_max().encode(&mut dst).expect("seven octets");
        assert_eq!(dst, [0x5F, 0xEA, 0xFB, 0x97, 0xFF, 0x0C, 0x63]);
    }

    #[test]
    fn cp56_decode_hand_written_vectors() {
        let decoded =
            Cp56Time2a::decode(&[0xE8, 0x03, 0x1E, 0x8C, 0x6F, 0x07, 0x18]).expect("valid octets");
        assert_eq!(decoded.milliseconds(), 1000);
        assert_eq!(decoded.minutes(), 30);
        assert_eq!(decoded.hours(), 12);
        assert_eq!(decoded.day_of_month(), 15);
        assert_eq!(decoded.day_of_week(), 3);
        assert_eq!(decoded.month(), 7);
        assert_eq!(decoded.year(), 24);
        assert!(decoded.is_summer_time());
        assert!(!decoded.is_invalid());
        assert!(!decoded.is_substituted());

        assert_eq!(
            Cp56Time2a::decode(&[0x00, 0x00, 0x00, 0x00, 0x01, 0x01, 0x00]),
            Some(cp56_min())
        );
        assert_eq!(
            Cp56Time2a::decode(&[0x5F, 0xEA, 0xFB, 0x97, 0xFF, 0x0C, 0x63]),
            Some(cp56_max())
        );
    }

    #[test]
    fn cp56_each_field_accepts_its_minimum_and_maximum() {
        let ok = |ms, mi, hr, dom, dow, mon, yr| Cp56Time2a::new(ms, mi, hr, dom, dow, mon, yr);
        assert!(ok(0, 0, 0, 1, 0, 1, 0).is_some());
        assert!(ok(0, 0, 0, 1, 1, 1, 0).is_some());
        assert!(ok(59_999, 59, 23, 31, 7, 12, 99).is_some());
        assert!(ok(59_999, 0, 0, 1, 7, 1, 0).is_some());
    }

    #[test]
    fn cp56_rejects_values_outside_each_range() {
        let ok = |ms, mi, hr, dom, dow, mon, yr| Cp56Time2a::new(ms, mi, hr, dom, dow, mon, yr);
        assert_eq!(ok(60_000, 0, 0, 1, 0, 1, 0), None);
        assert_eq!(ok(0, 60, 0, 1, 0, 1, 0), None);
        assert_eq!(ok(0, 0, 24, 1, 0, 1, 0), None);
        assert_eq!(ok(0, 0, 0, 0, 0, 1, 0), None);
        assert_eq!(ok(0, 0, 0, 32, 0, 1, 0), None);
        assert_eq!(ok(0, 0, 0, 1, 8, 1, 0), None);
        assert_eq!(ok(0, 0, 0, 1, 0, 0, 0), None);
        assert_eq!(ok(0, 0, 0, 1, 0, 13, 0), None);
        assert_eq!(ok(0, 0, 0, 1, 0, 1, 100), None);
    }

    #[test]
    fn cp56_round_trip_over_boundary_values() {
        for value in [cp56_min(), cp56_max()] {
            let mut dst = [0u8; 7];
            value.encode(&mut dst).expect("seven octets");
            assert_eq!(Cp56Time2a::decode(&dst), Some(value));
        }
    }

    #[test]
    fn cp56_reserved_bits_are_ignored_on_decode() {
        // The example vector with the reserved bits 30-31, 45-48 and 56 set:
        // 0x8C | 0x60, 0x07 | 0xF0 and 0x18 | 0x80.
        let with_reserved = [0xE8, 0x03, 0x1E, 0xEC, 0x6F, 0xF7, 0x98];
        assert_eq!(
            Cp56Time2a::decode(&with_reserved),
            Cp56Time2a::new(1000, 30, 12, 15, 3, 7, 24).map(|t| t.with_summer_time(true))
        );
    }

    #[test]
    fn cp56_rejects_truncated_input_and_out_of_range_wire_values() {
        assert_eq!(Cp56Time2a::decode(&[0x00; 6]), None);
        assert_eq!(Cp56Time2a::decode(&[]), None);
        // Milliseconds 60 000 on the wire.
        assert_eq!(
            Cp56Time2a::decode(&[0x60, 0xEA, 0x00, 0x00, 0x01, 0x01, 0x00]),
            None
        );
        // Day of month 0 on the wire.
        assert_eq!(
            Cp56Time2a::decode(&[0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00]),
            None
        );
    }

    #[test]
    fn cp56_encode_rejects_short_destination_without_partial_write() {
        let mut dst = [0xAAu8; 6];
        assert_eq!(cp56_max().encode(&mut dst), None);
        assert_eq!(dst, [0xAA; 6]);
    }

    #[test]
    fn cp24_hand_written_vectors() {
        let example = Cp24Time2a::new(1000, 30)
            .expect("in range")
            .with_substituted(true);
        let mut dst = [0u8; 3];
        example.encode(&mut dst).expect("three octets");
        assert_eq!(dst, [0xE8, 0x03, 0x5E]);

        Cp24Time2a::new(0, 0)
            .expect("in range")
            .encode(&mut dst)
            .expect("three octets");
        assert_eq!(dst, [0x00, 0x00, 0x00]);

        let max = Cp24Time2a::new(59_999, 59)
            .expect("in range")
            .with_invalid(true)
            .with_substituted(true);
        max.encode(&mut dst).expect("three octets");
        assert_eq!(dst, [0x5F, 0xEA, 0xFB]);
        assert_eq!(Cp24Time2a::decode(&dst), Some(max));
    }

    #[test]
    fn cp24_range_checks_and_truncation() {
        assert_eq!(Cp24Time2a::new(60_000, 0), None);
        assert_eq!(Cp24Time2a::new(0, 60), None);
        assert!(Cp24Time2a::new(59_999, 59).is_some());
        assert_eq!(Cp24Time2a::decode(&[0xE8, 0x03]), None);
    }

    #[test]
    fn cp16_hand_written_vectors_and_bounds() {
        let mut dst = [0u8; 2];
        Cp16Time2a::new(1000)
            .expect("in range")
            .encode(&mut dst)
            .expect("two octets");
        assert_eq!(dst, [0xE8, 0x03]);

        Cp16Time2a::new(59_999)
            .expect("in range")
            .encode(&mut dst)
            .expect("two octets");
        assert_eq!(dst, [0x5F, 0xEA]);

        assert_eq!(
            Cp16Time2a::decode(&dst).map(Cp16Time2a::milliseconds),
            Some(59_999)
        );
        assert_eq!(Cp16Time2a::new(60_000), None);
        assert_eq!(Cp16Time2a::decode(&[0x60, 0xEA]), None);
        assert_eq!(Cp16Time2a::decode(&[0x00]), None);
    }
}
