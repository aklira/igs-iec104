// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! Command and set-point elements (IEC 60870-5-101 7.2.6.15 to 7.2.6.17,
//! 7.2.6.26 and 7.2.6.39).

use super::{begin, field, field_u8, flag, put, put_flag};

const MAX_QOC_OUTPUT: u8 = 31;
const MAX_QOS_QUALIFIER: u8 = 127;

/// Qualifier of command QOC (101 7.2.6.26): the output mode and the select
/// flag. It occupies bits 3 to 8 of the command octet.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Qoc {
    output: u8,
    select: bool,
}

impl Qoc {
    /// QU 0: no additional definition.
    pub const NO_ADDITIONAL_DEFINITION: u8 = 0;
    /// QU 1: short pulse of a duration set in the outstation.
    pub const SHORT_PULSE: u8 = 1;
    /// QU 2: long pulse of a duration set in the outstation.
    pub const LONG_PULSE: u8 = 2;
    /// QU 3: persistent output.
    pub const PERSISTENT_OUTPUT: u8 = 3;

    /// Fails when `output` is above 31 (five bits).
    pub const fn new(output: u8, select: bool) -> Option<Self> {
        if output > MAX_QOC_OUTPUT {
            return None;
        }
        Some(Self { output, select })
    }

    /// The output mode QU, 0..=31.
    pub const fn output(self) -> u8 {
        self.output
    }

    /// True for select, false for execute (S/E).
    pub const fn select(self) -> bool {
        self.select
    }

    fn decode(src: &[u8]) -> Option<Self> {
        Self::new(field_u8(src, 2, 5)?, flag(src, 7)?)
    }

    fn encode(self, dst: &mut [u8]) -> Option<()> {
        put(dst, 2, 5, u64::from(self.output))?;
        put_flag(dst, 7, self.select)
    }
}

/// Single command SCO (101 7.2.6.15).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Sco {
    /// SCS: the command state is ON (`true`) or OFF (`false`).
    pub on: bool,
    /// The qualifier of command.
    pub qoc: Qoc,
}

impl Sco {
    /// Octets on the wire.
    pub const SIZE: usize = 1;

    /// Decodes the first octet of `src`.
    pub fn decode(src: &[u8]) -> Option<Self> {
        Some(Self {
            on: flag(src, 0)?,
            qoc: Qoc::decode(src)?,
        })
    }

    /// Writes the octet into the front of `dst`.
    pub fn encode(self, dst: &mut [u8]) -> Option<()> {
        begin(dst, Self::SIZE)?;
        put_flag(dst, 0, self.on)?;
        self.qoc.encode(dst)
    }
}

/// State of a double command (101 7.2.6.16). Codes 0 and 3 are not permitted
/// and are rejected on decoding.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum DoubleCommandState {
    /// Code 1: OFF.
    Off,
    /// Code 2: ON.
    On,
}

/// Double command DCO (101 7.2.6.16).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Dco {
    /// DCS: the requested state.
    pub state: DoubleCommandState,
    /// The qualifier of command.
    pub qoc: Qoc,
}

impl Dco {
    /// Octets on the wire.
    pub const SIZE: usize = 1;

    /// Decodes the first octet of `src`.
    pub fn decode(src: &[u8]) -> Option<Self> {
        let state = match field(src, 0, 2)? {
            1 => DoubleCommandState::Off,
            2 => DoubleCommandState::On,
            _ => return None,
        };
        Some(Self {
            state,
            qoc: Qoc::decode(src)?,
        })
    }

    /// Writes the octet into the front of `dst`.
    pub fn encode(self, dst: &mut [u8]) -> Option<()> {
        begin(dst, Self::SIZE)?;
        let code = match self.state {
            DoubleCommandState::Off => 1,
            DoubleCommandState::On => 2,
        };
        put(dst, 0, 2, code)?;
        self.qoc.encode(dst)
    }
}

/// Direction of a regulating step command (101 7.2.6.17). Codes 0 and 3 are
/// not permitted and are rejected on decoding.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum RegulatingStep {
    /// Code 1: next step LOWER.
    Lower,
    /// Code 2: next step HIGHER.
    Higher,
}

/// Regulating step command RCO (101 7.2.6.17).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Rco {
    /// RCS: the requested step.
    pub step: RegulatingStep,
    /// The qualifier of command.
    pub qoc: Qoc,
}

impl Rco {
    /// Octets on the wire.
    pub const SIZE: usize = 1;

    /// Decodes the first octet of `src`.
    pub fn decode(src: &[u8]) -> Option<Self> {
        let step = match field(src, 0, 2)? {
            1 => RegulatingStep::Lower,
            2 => RegulatingStep::Higher,
            _ => return None,
        };
        Some(Self {
            step,
            qoc: Qoc::decode(src)?,
        })
    }

    /// Writes the octet into the front of `dst`.
    pub fn encode(self, dst: &mut [u8]) -> Option<()> {
        begin(dst, Self::SIZE)?;
        let code = match self.step {
            RegulatingStep::Lower => 1,
            RegulatingStep::Higher => 2,
        };
        put(dst, 0, 2, code)?;
        self.qoc.encode(dst)
    }
}

/// Qualifier of set-point command QOS (101 7.2.6.39).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Qos {
    qualifier: u8,
    select: bool,
}

impl Qos {
    /// Octets on the wire.
    pub const SIZE: usize = 1;

    /// Fails when `qualifier` is above 127 (seven bits).
    pub const fn new(qualifier: u8, select: bool) -> Option<Self> {
        if qualifier > MAX_QOS_QUALIFIER {
            return None;
        }
        Some(Self { qualifier, select })
    }

    /// The qualifier QL, 0..=127; 0 is the default.
    pub const fn qualifier(self) -> u8 {
        self.qualifier
    }

    /// True for select, false for execute (S/E).
    pub const fn select(self) -> bool {
        self.select
    }

    /// Decodes the first octet of `src`.
    pub fn decode(src: &[u8]) -> Option<Self> {
        Self::new(field_u8(src, 0, 7)?, flag(src, 7)?)
    }

    /// Writes the octet into the front of `dst`.
    pub fn encode(self, dst: &mut [u8]) -> Option<()> {
        begin(dst, Self::SIZE)?;
        put(dst, 0, 7, u64::from(self.qualifier))?;
        put_flag(dst, 7, self.select)
    }
}
