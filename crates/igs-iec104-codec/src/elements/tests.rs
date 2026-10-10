// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! Round trips, hand-written byte vectors and range checks of every element.
//!
//! Vectors are computed from the bit positions of IEC 60870-5-101 7.2.6. Each
//! encode starts from a buffer full of 0xFF, so a vector also proves that the
//! reserved bits are written as zero.

use super::*;
use crate::formats::{Cp16Time2a, Cp56Time2a};
use std::fmt::Debug;

/// Encodes `value` into a dirty buffer of `size` octets.
fn wire<T>(value: T, encode: fn(T, &mut [u8]) -> Option<()>, size: usize) -> Vec<u8> {
    let mut dst = vec![0xFF; size];
    encode(value, &mut dst).expect("the buffer holds the element");
    dst
}

/// Encodes `value`, checks the bytes, and decodes them back to `value`.
fn check<T: Copy + PartialEq + Debug>(
    value: T,
    bytes: &[u8],
    encode: fn(T, &mut [u8]) -> Option<()>,
    decode: fn(&[u8]) -> Option<T>,
) {
    assert_eq!(
        wire(value, encode, bytes.len()),
        bytes,
        "encoding of {value:?}"
    );
    assert_eq!(decode(bytes), Some(value), "decoding of {bytes:02X?}");
}

/// Every element rejects an empty buffer.
fn check_truncated<T>(decode: fn(&[u8]) -> Option<T>, size: usize) {
    assert!(decode(&[]).is_none());
    assert!(decode(&vec![0; size - 1]).is_none());
}

fn qf(blocked: bool, substituted: bool, not_topical: bool, invalid: bool) -> QualityFlags {
    QualityFlags {
        blocked,
        substituted,
        not_topical,
        invalid,
    }
}

const NONE: QualityFlags = QualityFlags {
    blocked: false,
    substituted: false,
    not_topical: false,
    invalid: false,
};

#[test]
fn quality_and_point_elements() {
    // QDS: overflow = bit 1, not topical = bit 7.
    check(
        Qds {
            overflow: true,
            quality: qf(false, false, true, false),
        },
        &[0x41],
        Qds::encode,
        Qds::decode,
    );
    check(Qds::default(), &[0x00], Qds::encode, Qds::decode);

    // SIQ: ON = bit 1, blocked = bit 5, invalid = bit 8.
    check(
        Siq {
            on: true,
            quality: qf(true, false, false, true),
        },
        &[0x91],
        Siq::encode,
        Siq::decode,
    );
    check(
        Siq {
            on: false,
            quality: NONE,
        },
        &[0x00],
        Siq::encode,
        Siq::decode,
    );

    // DIQ: state ON = code 2 in bits 1-2, substituted = bit 6.
    check(
        Diq {
            state: DoublePoint::On,
            quality: qf(false, true, false, false),
        },
        &[0x22],
        Diq::encode,
        Diq::decode,
    );
    check(
        Diq {
            state: DoublePoint::Indeterminate,
            quality: NONE,
        },
        &[0x03],
        Diq::encode,
        Diq::decode,
    );

    // QDP: elapsed time invalid = bit 4, blocked = bit 5.
    check(
        Qdp {
            elapsed_invalid: true,
            quality: qf(true, false, false, false),
        },
        &[0x18],
        Qdp::encode,
        Qdp::decode,
    );

    // SEP: state indeterminate = code 3, elapsed invalid = bit 4, invalid = bit 8.
    check(
        Sep {
            state: DoublePoint::Indeterminate,
            elapsed_invalid: true,
            quality: qf(false, false, false, true),
        },
        &[0x8B],
        Sep::encode,
        Sep::decode,
    );

    // Codes 0 and 3 stay distinct on the wire.
    check(
        Diq {
            state: DoublePoint::IndeterminateOrIntermediate,
            quality: NONE,
        },
        &[0x00],
        Diq::encode,
        Diq::decode,
    );
    assert_eq!(
        Diq::decode(&[0x00]).map(|d| d.state),
        Some(DoublePoint::IndeterminateOrIntermediate)
    );
}

#[test]
fn quality_element_sizes_and_truncation() {
    check_truncated(Siq::decode, Siq::SIZE);
    check_truncated(Diq::decode, Diq::SIZE);
    check_truncated(Qds::decode, Qds::SIZE);
    check_truncated(Qdp::decode, Qdp::SIZE);
    check_truncated(Sep::decode, Sep::SIZE);
    assert_eq!(
        (Siq::SIZE, Diq::SIZE, Qds::SIZE, Qdp::SIZE, Sep::SIZE),
        (1, 1, 1, 1, 1)
    );
}

#[test]
fn vti_range_and_vectors() {
    // Seven-bit two's complement, transient = bit 8.
    check(
        Vti::new(-1, true).expect("in range"),
        &[0xFF],
        Vti::encode,
        Vti::decode,
    );
    check(
        Vti::new(-64, false).expect("in range"),
        &[0x40],
        Vti::encode,
        Vti::decode,
    );
    check(
        Vti::new(63, false).expect("in range"),
        &[0x3F],
        Vti::encode,
        Vti::decode,
    );
    check(
        Vti::new(0, true).expect("in range"),
        &[0x80],
        Vti::encode,
        Vti::decode,
    );

    assert!(Vti::new(-65, false).is_none());
    assert!(Vti::new(64, false).is_none());
    // 0x7F is -1 in seven bits.
    assert_eq!(Vti::decode(&[0x7F]).map(Vti::value), Some(-1));
    check_truncated(Vti::decode, Vti::SIZE);
}

#[test]
fn command_elements() {
    // SCO: ON = bit 1, output mode 1 = bit 3, select = bit 8.
    check(
        Sco {
            on: true,
            qoc: Qoc::new(1, true).expect("in range"),
        },
        &[0x85],
        Sco::encode,
        Sco::decode,
    );
    // DCO: OFF = code 1, output mode 0, execute.
    check(
        Dco {
            state: DoubleCommandState::Off,
            qoc: Qoc::new(Qoc::NO_ADDITIONAL_DEFINITION, false).expect("in range"),
        },
        &[0x01],
        Dco::encode,
        Dco::decode,
    );
    // RCO: HIGHER = code 2, output mode 3 = bits 3-4.
    check(
        Rco {
            step: RegulatingStep::Higher,
            qoc: Qoc::new(Qoc::PERSISTENT_OUTPUT, false).expect("in range"),
        },
        &[0x0E],
        Rco::encode,
        Rco::decode,
    );
    // QOS: qualifier 5, select.
    check(
        Qos::new(5, true).expect("in range"),
        &[0x85],
        Qos::encode,
        Qos::decode,
    );
    check(
        Qos::new(127, false).expect("in range"),
        &[0x7F],
        Qos::encode,
        Qos::decode,
    );
}

#[test]
fn command_range_checks_and_invalid_codes() {
    assert!(Qoc::new(31, true).is_some());
    assert!(Qoc::new(32, true).is_none());
    assert!(Qos::new(128, false).is_none());
    // Codes 0 and 3 are not permitted in DCO and RCO.
    assert!(Dco::decode(&[0x00]).is_none());
    assert!(Dco::decode(&[0x03]).is_none());
    assert!(Rco::decode(&[0x00]).is_none());
    assert!(Rco::decode(&[0x03]).is_none());
    check_truncated(Sco::decode, Sco::SIZE);
    check_truncated(Dco::decode, Dco::SIZE);
    check_truncated(Rco::decode, Rco::SIZE);
    check_truncated(Qos::decode, Qos::SIZE);
}

#[test]
fn system_qualifiers() {
    // COI: cause 2 (remote reset), parameters changed = bit 8.
    check(
        Coi::new(2, true).expect("in range"),
        &[0x82],
        Coi::encode,
        Coi::decode,
    );
    check(
        Coi::new(127, false).expect("in range"),
        &[0x7F],
        Coi::encode,
        Coi::decode,
    );
    assert!(Coi::new(128, false).is_none());

    // QOI: station interrogation is 20, group 1 is 21, group 16 is 36.
    check(Qoi::STATION, &[20], Qoi::encode, Qoi::decode);
    check(
        Qoi::group(1).expect("1..=16"),
        &[21],
        Qoi::encode,
        Qoi::decode,
    );
    check(
        Qoi::group(16).expect("1..=16"),
        &[36],
        Qoi::encode,
        Qoi::decode,
    );
    assert!(Qoi::group(0).is_none());
    assert!(Qoi::group(17).is_none());

    // QCC: request 5 in bits 1-6, freeze reset (code 3) in bits 7-8.
    check(
        Qcc::new(5, CounterFreeze::Reset).expect("in range"),
        &[0xC5],
        Qcc::encode,
        Qcc::decode,
    );
    check(
        Qcc::new(63, CounterFreeze::Read).expect("in range"),
        &[0x3F],
        Qcc::encode,
        Qcc::decode,
    );
    assert!(Qcc::new(64, CounterFreeze::Read).is_none());

    // QPM: kind 4 in bits 1-6; LPC and POP (bits 7-8) are written as zero.
    check(
        Qpm::new(4).expect("in range"),
        &[0x04],
        Qpm::encode,
        Qpm::decode,
    );
    assert!(Qpm::new(64).is_none());
    assert_eq!(Qpm::decode(&[0xC4]).map(Qpm::kind), Some(4));

    check(Qpa::new(255), &[0xFF], Qpa::encode, Qpa::decode);
    check(Qrp::GENERAL_RESET, &[0x01], Qrp::encode, Qrp::decode);
    check(Tsc::new(0x1234), &[0x34, 0x12], Tsc::encode, Tsc::decode);

    check_truncated(Coi::decode, Coi::SIZE);
    check_truncated(Qcc::decode, Qcc::SIZE);
    check_truncated(Qpm::decode, Qpm::SIZE);
    check_truncated(Qoi::decode, Qoi::SIZE);
    check_truncated(Tsc::decode, Tsc::SIZE);
}

#[test]
fn counter_and_status_elements() {
    // BCR: counter 0x01020304 little-endian, sequence 5 in bits 33-37.
    check(
        Bcr::new(0x0102_0304, 5).expect("in range"),
        &[0x04, 0x03, 0x02, 0x01, 0x05],
        Bcr::encode,
        Bcr::decode,
    );

    // Carry = bit 38, adjusted = bit 39, invalid = bit 40 of the counter.
    let mut flagged = Bcr::new(-1, 31).expect("in range");
    flagged.carry = true;
    flagged.invalid = true;
    check(
        flagged,
        &[0xFF, 0xFF, 0xFF, 0xFF, 0xBF],
        Bcr::encode,
        Bcr::decode,
    );
    assert!(Bcr::new(0, 32).is_none());

    // SCD: point 1 status, point 16 change detection (bit 32).
    let scd = Scd::new()
        .with_status(1, true)
        .expect("1..=16")
        .with_changed(16, true)
        .expect("1..=16");
    check(scd, &[0x01, 0x00, 0x00, 0x80], Scd::encode, Scd::decode);
    assert_eq!(scd.status(1), Some(true));
    assert_eq!(scd.status(2), Some(false));
    assert_eq!(scd.changed(16), Some(true));
    assert!(scd.status(0).is_none());
    assert!(scd.status(17).is_none());
    assert!(scd.with_changed(17, true).is_none());

    // BSI: bit 1 and bit 32.
    let bsi = Bsi::new()
        .with_bit(1, true)
        .expect("1..=32")
        .with_bit(32, true)
        .expect("1..=32");
    check(bsi, &[0x01, 0x00, 0x00, 0x80], Bsi::encode, Bsi::decode);
    assert_eq!(bsi.bit(32), Some(true));
    assert!(bsi.bit(33).is_none());

    // SPE: general start = bit 1, reverse direction = bit 6.
    check(
        Spe {
            general_start: true,
            reverse_direction: true,
            ..Spe::default()
        },
        &[0x21],
        Spe::encode,
        Spe::decode,
    );
    // OCI: general command = bit 1, phase L3 = bit 4.
    check(
        Oci {
            general_command: true,
            phase_l3: true,
            ..Oci::default()
        },
        &[0x09],
        Oci::encode,
        Oci::decode,
    );

    check_truncated(Bcr::decode, Bcr::SIZE);
    check_truncated(Scd::decode, Scd::SIZE);
    check_truncated(Bsi::decode, Bsi::SIZE);
    check_truncated(Spe::decode, Spe::SIZE);
    check_truncated(Oci::decode, Oci::SIZE);
}

#[test]
fn range_time_unbounded_and_bounded() {
    check(
        RangeTime::Unbounded,
        &[0; 7],
        RangeTime::encode,
        RangeTime::decode,
    );

    // A bounded time: 1000 ms, 30 min, 12 h, 15th, Wednesday, July, year 24.
    let bounded = Cp56Time2a::new(1000, 30, 12, 15, 3, 7, 24).expect("in range");
    check(
        RangeTime::At(bounded),
        &[0xE8, 0x03, 0x1E, 0x0C, 0x6F, 0x07, 0x18],
        RangeTime::encode,
        RangeTime::decode,
    );
    // An invalid time that is not all zeros is still rejected.
    assert!(RangeTime::decode(&[0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00]).is_none());
    check_truncated(RangeTime::decode, RangeTime::SIZE);
}

#[test]
fn file_elements() {
    check(Nof::new(0x1234), &[0x34, 0x12], Nof::encode, Nof::decode);
    check(Nos::new(255), &[0xFF], Nos::encode, Nos::decode);
    check(Los::new(7), &[0x07], Los::encode, Los::decode);
    check(Chs::new(0xAB), &[0xAB], Chs::encode, Chs::decode);
    check(
        Lsq::SECTION_WITH_DEACTIVATION,
        &[0x04],
        Lsq::encode,
        Lsq::decode,
    );
    check(
        Lof::new(0x0001_0203).expect("24 bits"),
        &[0x03, 0x02, 0x01],
        Lof::encode,
        Lof::decode,
    );
    check(
        Lof::new(0x00FF_FFFF).expect("24 bits"),
        &[0xFF, 0xFF, 0xFF],
        Lof::encode,
        Lof::decode,
    );
    assert!(Lof::new(0x0100_0000).is_none());

    // SOF: status 3, last in directory = bit 6, transfer active = bit 8.
    let mut sof = Sof::new(3).expect("in range");
    sof.last_in_directory = true;
    sof.transfer_active = true;
    check(sof, &[0xA3], Sof::encode, Sof::decode);
    assert!(Sof::new(32).is_none());

    // FRQ: qualifier 1, negative confirm = bit 8. SRQ: qualifier 127.
    check(
        Frq::new(1, true).expect("in range"),
        &[0x81],
        Frq::encode,
        Frq::decode,
    );
    check(
        Srq::new(127, false).expect("in range"),
        &[0x7F],
        Srq::encode,
        Srq::decode,
    );
    assert!(Frq::new(128, false).is_none());

    // SCQ: command 1 in the low nibble, error 2 in the high nibble. AFQ: 3, 0.
    check(
        Scq::new(1, 2).expect("in range"),
        &[0x21],
        Scq::encode,
        Scq::decode,
    );
    check(
        Afq::new(3, 0).expect("in range"),
        &[0x03],
        Afq::encode,
        Afq::decode,
    );
    assert!(Scq::new(16, 0).is_none());
    assert!(Afq::new(0, 16).is_none());
    assert_eq!(
        Scq::decode(&[0x21]).map(|s| (s.command(), s.error())),
        Some((1, 2))
    );

    check_truncated(Nof::decode, Nof::SIZE);
    check_truncated(Lof::decode, Lof::SIZE);
    check_truncated(Sof::decode, Sof::SIZE);
    check_truncated(Scq::decode, Scq::SIZE);
}

#[test]
fn segment_copies_the_length_given_by_los() {
    let segment = Segment::decode(&[1, 2, 3, 4], 2).expect("two octets available");
    assert_eq!(segment.data(), &[1, 2]);
    assert!(Segment::decode(&[1, 2], 3).is_none());

    let mut dst = [0xFF; 4];
    segment.encode(&mut dst).expect("fits");
    assert_eq!(dst, [1, 2, 0xFF, 0xFF]);
    let mut short = [0u8; 1];
    assert!(segment.encode(&mut short).is_none());
}

#[test]
fn reused_formats_keep_their_sizes() {
    assert_eq!(
        (Nva::SIZE, Sva::SIZE, ShortFloat::SIZE, Cp16Time2a::SIZE),
        (2, 2, 4, 2)
    );
}

#[test]
fn encoders_fail_on_short_buffers() {
    assert!(Siq {
        on: true,
        quality: NONE
    }
    .encode(&mut [])
    .is_none());
    assert!(Bcr::new(1, 1)
        .expect("in range")
        .encode(&mut [0; 4])
        .is_none());
    assert!(Scd::new().encode(&mut [0; 3]).is_none());
    assert!(RangeTime::Unbounded.encode(&mut [0; 6]).is_none());
}
