// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! Counters, status bit strings and protection outputs (IEC 60870-5-101
//! 7.2.6.9, 7.2.6.11 to 7.2.6.13 and 7.2.6.40).

use super::{begin, field, flag, put, put_flag};

const MAX_SEQUENCE: u8 = 31;
const STATUS_POINTS: u8 = 16;
const BSI_POINTS: u8 = 32;
/// Bit index of the change-detection flag of point 1 in an SCD.
const CHANGE_OFFSET: usize = 16;

/// Zero-based index of point `point` (1-based) in a bit string of `points` bits.
fn point_index(point: u8, points: u8) -> Option<usize> {
    let index = point.checked_sub(1)?;
    (index < points).then_some(usize::from(index))
}

/// Bit index of the change-detection flag of point `point` in an SCD.
fn change_index(point: u8) -> Option<usize> {
    point_index(point, STATUS_POINTS)?.checked_add(CHANGE_OFFSET)
}

fn bit_at(bits: u32, index: usize) -> bool {
    (bits >> index) & 1 == 1
}

fn with_bit_at(bits: u32, index: usize, on: bool) -> u32 {
    let mask = 1u32 << index;
    if on {
        bits | mask
    } else {
        bits & !mask
    }
}

/// Binary counter reading BCR (101 7.2.6.9): a signed 32-bit counter with its
/// sequence notation (SQ, CY, CA, IV).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Bcr {
    counter: i32,
    sequence: u8,
    /// CY: the counter overflowed in the integration period.
    pub carry: bool,
    /// CA: the counter was adjusted since the last reading.
    pub adjusted: bool,
    /// IV: the counter reading is invalid.
    pub invalid: bool,
}

impl Bcr {
    /// Octets on the wire.
    pub const SIZE: usize = 5;

    /// Fails when `sequence` is above 31 (five bits). The flags start cleared.
    pub const fn new(counter: i32, sequence: u8) -> Option<Self> {
        if sequence > MAX_SEQUENCE {
            return None;
        }
        Some(Self {
            counter,
            sequence,
            carry: false,
            adjusted: false,
            invalid: false,
        })
    }

    /// The counter reading.
    pub const fn counter(self) -> i32 {
        self.counter
    }

    /// The sequence number SQ, 0..=31.
    pub const fn sequence(self) -> u8 {
        self.sequence
    }

    /// Decodes the first five octets of `src`.
    pub fn decode(src: &[u8]) -> Option<Self> {
        let counter = u32::try_from(field(src, 0, 32)?).ok()? as i32;
        let sequence = u8::try_from(field(src, 32, 5)?).ok()?;
        Some(Self {
            carry: flag(src, 37)?,
            adjusted: flag(src, 38)?,
            invalid: flag(src, 39)?,
            ..Self::new(counter, sequence)?
        })
    }

    /// Writes the five octets into the front of `dst`.
    pub fn encode(self, dst: &mut [u8]) -> Option<()> {
        begin(dst, Self::SIZE)?;
        put(dst, 0, 32, u64::from(self.counter as u32))?;
        put(dst, 32, 5, u64::from(self.sequence))?;
        put_flag(dst, 37, self.carry)?;
        put_flag(dst, 38, self.adjusted)?;
        put_flag(dst, 39, self.invalid)
    }
}

/// Status and status change detection SCD (101 7.2.6.40): sixteen status bits
/// and sixteen change-detection bits.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct Scd {
    bits: u32,
}

impl Scd {
    /// Octets on the wire.
    pub const SIZE: usize = 4;

    /// All status points OFF and no change detected.
    pub const fn new() -> Self {
        Self { bits: 0 }
    }

    /// The status of point `point` (1..=16): `true` for ON.
    pub fn status(self, point: u8) -> Option<bool> {
        Some(bit_at(self.bits, point_index(point, STATUS_POINTS)?))
    }

    /// The change-detection flag of point `point` (1..=16).
    pub fn changed(self, point: u8) -> Option<bool> {
        Some(bit_at(self.bits, change_index(point)?))
    }

    /// Sets the status of point `point` (1..=16).
    pub fn with_status(self, point: u8, on: bool) -> Option<Self> {
        let index = point_index(point, STATUS_POINTS)?;
        Some(Self {
            bits: with_bit_at(self.bits, index, on),
        })
    }

    /// Sets the change-detection flag of point `point` (1..=16).
    pub fn with_changed(self, point: u8, changed: bool) -> Option<Self> {
        Some(Self {
            bits: with_bit_at(self.bits, change_index(point)?, changed),
        })
    }

    /// Decodes the first four octets of `src`.
    pub fn decode(src: &[u8]) -> Option<Self> {
        Some(Self {
            bits: u32::try_from(field(src, 0, 32)?).ok()?,
        })
    }

    /// Writes the four octets into the front of `dst`.
    pub fn encode(self, dst: &mut [u8]) -> Option<()> {
        begin(dst, Self::SIZE)?;
        put(dst, 0, 32, u64::from(self.bits))
    }
}

/// Binary state information BSI (101 7.2.6.13): 32 independent state bits.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct Bsi {
    bits: u32,
}

impl Bsi {
    /// Octets on the wire.
    pub const SIZE: usize = 4;

    /// All bits cleared.
    pub const fn new() -> Self {
        Self { bits: 0 }
    }

    /// The state of bit `number` (1..=32).
    pub fn bit(self, number: u8) -> Option<bool> {
        Some(bit_at(self.bits, point_index(number, BSI_POINTS)?))
    }

    /// Sets the state of bit `number` (1..=32).
    pub fn with_bit(self, number: u8, on: bool) -> Option<Self> {
        let index = point_index(number, BSI_POINTS)?;
        Some(Self {
            bits: with_bit_at(self.bits, index, on),
        })
    }

    /// Decodes the first four octets of `src`.
    pub fn decode(src: &[u8]) -> Option<Self> {
        Some(Self {
            bits: u32::try_from(field(src, 0, 32)?).ok()?,
        })
    }

    /// Writes the four octets into the front of `dst`.
    pub fn encode(self, dst: &mut [u8]) -> Option<()> {
        begin(dst, Self::SIZE)?;
        put(dst, 0, 32, u64::from(self.bits))
    }
}

/// Start events of protection equipment SPE (101 7.2.6.11).
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct Spe {
    /// GS: general start of operation.
    pub general_start: bool,
    /// SL1: start of operation in phase L1.
    pub phase_l1: bool,
    /// SL2: start of operation in phase L2.
    pub phase_l2: bool,
    /// SL3: start of operation in phase L3.
    pub phase_l3: bool,
    /// SIE: start of operation of the earth current.
    pub earth_current: bool,
    /// SRD: start of operation in reverse direction.
    pub reverse_direction: bool,
}

impl Spe {
    /// Octets on the wire.
    pub const SIZE: usize = 1;

    /// Decodes the first octet of `src`.
    pub fn decode(src: &[u8]) -> Option<Self> {
        Some(Self {
            general_start: flag(src, 0)?,
            phase_l1: flag(src, 1)?,
            phase_l2: flag(src, 2)?,
            phase_l3: flag(src, 3)?,
            earth_current: flag(src, 4)?,
            reverse_direction: flag(src, 5)?,
        })
    }

    /// Writes the octet into the front of `dst`.
    pub fn encode(self, dst: &mut [u8]) -> Option<()> {
        begin(dst, Self::SIZE)?;
        put_flag(dst, 0, self.general_start)?;
        put_flag(dst, 1, self.phase_l1)?;
        put_flag(dst, 2, self.phase_l2)?;
        put_flag(dst, 3, self.phase_l3)?;
        put_flag(dst, 4, self.earth_current)?;
        put_flag(dst, 5, self.reverse_direction)
    }
}

/// Output circuit information of protection equipment OCI (101 7.2.6.12).
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct Oci {
    /// GC: general command to the output circuit.
    pub general_command: bool,
    /// CL1: command to the output circuit in phase L1.
    pub phase_l1: bool,
    /// CL2: command to the output circuit in phase L2.
    pub phase_l2: bool,
    /// CL3: command to the output circuit in phase L3.
    pub phase_l3: bool,
}

impl Oci {
    /// Octets on the wire.
    pub const SIZE: usize = 1;

    /// Decodes the first octet of `src`.
    pub fn decode(src: &[u8]) -> Option<Self> {
        Some(Self {
            general_command: flag(src, 0)?,
            phase_l1: flag(src, 1)?,
            phase_l2: flag(src, 2)?,
            phase_l3: flag(src, 3)?,
        })
    }

    /// Writes the octet into the front of `dst`.
    pub fn encode(self, dst: &mut [u8]) -> Option<()> {
        begin(dst, Self::SIZE)?;
        put_flag(dst, 0, self.general_command)?;
        put_flag(dst, 1, self.phase_l1)?;
        put_flag(dst, 2, self.phase_l2)?;
        put_flag(dst, 3, self.phase_l3)
    }
}
