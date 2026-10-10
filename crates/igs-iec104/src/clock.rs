// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! The local time as CP56Time2a (101 7.2.6.18), in UTC.
//!
//! A clock synchronization carries the time of the controlling station (§7.6), and the
//! controlled station answers with its own time from before the synchronization. CP56Time2a
//! stores the years 0 to 99 only, so these functions accept the years 2000 to 2099.

use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};

use igs_iec104_codec::formats::Cp56Time2a;

const DAY: u64 = 86_400;

/// A time that CP56Time2a cannot hold, or a clock that cannot be read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClockError {
    /// The clock is before 1970.
    BeforeEpoch,
    /// The year is outside 2000 to 2099.
    Years,
    /// The time is out of the range of the fields of CP56Time2a.
    Range,
}

impl fmt::Display for ClockError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BeforeEpoch => write!(f, "the clock is before 1970"),
            Self::Years => write!(
                f,
                "the clock is outside the years 2000 to 2099 of CP56Time2a"
            ),
            Self::Range => write!(f, "the time is out of the range of CP56Time2a"),
        }
    }
}

impl std::error::Error for ClockError {}

/// The current time of this machine, in UTC.
pub fn now_utc() -> Result<Cp56Time2a, ClockError> {
    let since = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| ClockError::BeforeEpoch)?;
    let millis = u16::try_from(since.subsec_millis()).map_err(|_| ClockError::Range)?;
    cp56_of_unix(since.as_secs(), millis)
}

/// The CP56Time2a of a Unix time, in seconds plus milliseconds, in UTC.
pub fn cp56_of_unix(seconds: u64, millis: u16) -> Result<Cp56Time2a, ClockError> {
    let mut days = seconds / DAY;
    let of_day = seconds % DAY;
    // 1970-01-01 was a Thursday; CP56Time2a numbers the days from 1 (Monday) to 7 (Sunday).
    let weekday = days.checked_add(3).ok_or(ClockError::Range)? % 7;
    let day_of_week = u8::try_from(weekday)
        .map_err(|_| ClockError::Range)?
        .checked_add(1)
        .ok_or(ClockError::Range)?;
    let mut year = 1970u64;
    loop {
        let length = if is_leap(year) { 366 } else { 365 };
        if days < length {
            break;
        }
        days = days.checked_sub(length).ok_or(ClockError::Range)?;
        year = year.checked_add(1).ok_or(ClockError::Range)?;
    }
    let mut month = 1u64;
    loop {
        let length = days_in_month(year, month);
        if days < length {
            break;
        }
        days = days.checked_sub(length).ok_or(ClockError::Range)?;
        month = month.checked_add(1).ok_or(ClockError::Range)?;
    }
    if !(2000..=2099).contains(&year) {
        return Err(ClockError::Years);
    }
    let day_of_month = days.checked_add(1).ok_or(ClockError::Range)?;
    let hours = of_day / 3600;
    let minutes = (of_day % 3600) / 60;
    let seconds_of_minute = u16::try_from(of_day % 60).map_err(|_| ClockError::Range)?;
    let milliseconds = seconds_of_minute
        .checked_mul(1000)
        .and_then(|value| value.checked_add(millis))
        .ok_or(ClockError::Range)?;
    Cp56Time2a::new(
        milliseconds,
        u8::try_from(minutes).map_err(|_| ClockError::Range)?,
        u8::try_from(hours).map_err(|_| ClockError::Range)?,
        u8::try_from(day_of_month).map_err(|_| ClockError::Range)?,
        day_of_week,
        u8::try_from(month).map_err(|_| ClockError::Range)?,
        u8::try_from(year % 100).map_err(|_| ClockError::Range)?,
    )
    .ok_or(ClockError::Range)
}

fn is_leap(year: u64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn days_in_month(year: u64, month: u64) -> u64 {
    match month {
        2 if is_leap(year) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn a_unix_time_becomes_the_utc_cp56_time() {
        // 2026-10-10 12:34:56 UTC, a Saturday.
        let time = cp56_of_unix(1_791_635_696, 789).expect("in range");
        assert_eq!(time.milliseconds(), 56_789);
        assert_eq!(time.minutes(), 34);
        assert_eq!(time.hours(), 12);
        assert_eq!(time.day_of_month(), 10);
        assert_eq!(time.day_of_week(), 6);
        assert_eq!(time.month(), 10);
        assert_eq!(time.year(), 26);
    }

    #[test]
    fn leap_days_and_the_end_of_the_year_are_counted() {
        let leap = cp56_of_unix(1_709_164_800, 0).expect("in range");
        assert_eq!(
            (leap.month(), leap.day_of_month(), leap.year()),
            (2, 29, 24)
        );
        let last = cp56_of_unix(1_767_225_599, 0).expect("in range");
        assert_eq!(
            (last.month(), last.day_of_month(), last.year()),
            (12, 31, 25)
        );
    }

    #[test]
    fn years_outside_2000_to_2099_are_refused() {
        // 1999-12-31 23:59:59 UTC.
        assert_eq!(cp56_of_unix(946_684_799, 0), Err(ClockError::Years));
    }
}
