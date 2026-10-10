// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! System information and qualifiers (IEC 60870-5-101 7.2.6.21 to 7.2.6.27)
//! and the test sequence counter of C_TS_TA_1 (IEC 60870-5-104 8.8).

use super::{begin, field, field_u8, flag, put, put_flag};

const MAX_CAUSE: u8 = 127;
const MAX_REQUEST: u8 = 63;
const MAX_PARAMETER_KIND: u8 = 63;

/// Station interrogation (global) as a qualifier of interrogation (101 7.2.6.22).
const STATION_INTERROGATION: u8 = 20;

unsigned_element! {
    /// Qualifier of interrogation QOI (101 7.2.6.22). Use [`Qoi::STATION`] or
    /// [`Qoi::group`].
    Qoi, u8, size: 1, bits: 8
}

impl Qoi {
    /// Station interrogation (global), code 20.
    pub const STATION: Self = Self::new(STATION_INTERROGATION);

    /// Interrogation of group 1 to 16 (codes 21 to 36). Fails for other numbers.
    pub fn group(number: u8) -> Option<Self> {
        if !(1..=16).contains(&number) {
            return None;
        }
        Some(Self::new(STATION_INTERROGATION.checked_add(number)?))
    }
}

unsigned_element! {
    /// Qualifier of parameter activation QPA (101 7.2.6.25).
    Qpa, u8, size: 1, bits: 8
}

impl Qpa {
    /// Act/deact of the previously loaded parameters (object address 0).
    pub const PREVIOUSLY_LOADED: Self = Self::new(1);
    /// Act/deact of the parameter of the addressed object.
    pub const ADDRESSED_OBJECT: Self = Self::new(2);
    /// Act/deact of persistent cyclic or periodic transmission of the object.
    pub const PERIODIC_TRANSMISSION: Self = Self::new(3);
}

unsigned_element! {
    /// Qualifier of reset process command QRP (101 7.2.6.27).
    Qrp, u8, size: 1, bits: 8
}

impl Qrp {
    /// General reset of process.
    pub const GENERAL_RESET: Self = Self::new(1);
    /// Reset of pending information with time tag of the event buffer.
    pub const RESET_PENDING_EVENTS: Self = Self::new(2);
}

unsigned_element! {
    /// Test sequence counter TSC of C_TS_TA_1 (IEC 60870-5-104 8.8): any value
    /// is allowed, the response repeats the request's value.
    Tsc, u16, size: 2, bits: 16
}

/// Cause of initialization COI (101 7.2.6.21).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Coi {
    cause: u8,
    /// True when the initialization happened after a change of local
    /// parameters, false when the parameters are unchanged.
    pub local_parameters_changed: bool,
}

impl Coi {
    /// Octets on the wire.
    pub const SIZE: usize = 1;

    /// Local power switch on: cause 0.
    pub const LOCAL_POWER_ON: u8 = 0;
    /// Local manual reset: cause 1.
    pub const LOCAL_MANUAL_RESET: u8 = 1;
    /// Remote reset: cause 2.
    pub const REMOTE_RESET: u8 = 2;

    /// Fails when `cause` is above 127 (seven bits).
    pub const fn new(cause: u8, local_parameters_changed: bool) -> Option<Self> {
        if cause > MAX_CAUSE {
            return None;
        }
        Some(Self {
            cause,
            local_parameters_changed,
        })
    }

    /// The cause, 0..=127.
    pub const fn cause(self) -> u8 {
        self.cause
    }

    /// Decodes the first octet of `src`.
    pub fn decode(src: &[u8]) -> Option<Self> {
        Self::new(field_u8(src, 0, 7)?, flag(src, 7)?)
    }

    /// Writes the octet into the front of `dst`.
    pub fn encode(self, dst: &mut [u8]) -> Option<()> {
        begin(dst, Self::SIZE)?;
        put(dst, 0, 7, u64::from(self.cause))?;
        put_flag(dst, 7, self.local_parameters_changed)
    }
}

/// Freeze action of a counter interrogation (101 7.2.6.23), coded in two bits.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum CounterFreeze {
    /// Code 0: read, no freeze or reset.
    Read,
    /// Code 1: freeze without reset; the frozen value is the integrated total.
    FreezeWithoutReset,
    /// Code 2: freeze with reset; the frozen value is the incremental information.
    FreezeWithReset,
    /// Code 3: counter reset.
    Reset,
}

impl CounterFreeze {
    fn from_code(code: u64) -> Option<Self> {
        match code {
            0 => Some(Self::Read),
            1 => Some(Self::FreezeWithoutReset),
            2 => Some(Self::FreezeWithReset),
            3 => Some(Self::Reset),
            _ => None,
        }
    }

    const fn code(self) -> u64 {
        match self {
            Self::Read => 0,
            Self::FreezeWithoutReset => 1,
            Self::FreezeWithReset => 2,
            Self::Reset => 3,
        }
    }
}

/// Qualifier of counter interrogation command QCC (101 7.2.6.23).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Qcc {
    request: u8,
    freeze: CounterFreeze,
}

impl Qcc {
    /// Octets on the wire.
    pub const SIZE: usize = 1;

    /// Fails when `request` is above 63 (six bits).
    pub const fn new(request: u8, freeze: CounterFreeze) -> Option<Self> {
        if request > MAX_REQUEST {
            return None;
        }
        Some(Self { request, freeze })
    }

    /// The counter group requested, 0..=63 (0 is not used).
    pub const fn request(self) -> u8 {
        self.request
    }

    /// The freeze action.
    pub const fn freeze(self) -> CounterFreeze {
        self.freeze
    }

    /// Decodes the first octet of `src`.
    pub fn decode(src: &[u8]) -> Option<Self> {
        Self::new(
            field_u8(src, 0, 6)?,
            CounterFreeze::from_code(field(src, 6, 2)?)?,
        )
    }

    /// Writes the octet into the front of `dst`.
    pub fn encode(self, dst: &mut [u8]) -> Option<()> {
        begin(dst, Self::SIZE)?;
        put(dst, 0, 6, u64::from(self.request))?;
        put(dst, 6, 2, self.freeze.code())
    }
}

/// Qualifier of parameter of measured values QPM (101 7.2.6.24). The LPC and
/// POP bits are not used by the standard: they are written as 0 and ignored.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Qpm {
    kind: u8,
}

impl Qpm {
    /// Octets on the wire.
    pub const SIZE: usize = 1;

    /// Kind 1: threshold value.
    pub const THRESHOLD: u8 = 1;
    /// Kind 2: smoothing factor (filter time constant).
    pub const SMOOTHING_FACTOR: u8 = 2;
    /// Kind 3: low limit for transmission of measured values.
    pub const LOW_LIMIT: u8 = 3;
    /// Kind 4: high limit for transmission of measured values.
    pub const HIGH_LIMIT: u8 = 4;

    /// Fails when `kind` is above 63 (six bits).
    pub const fn new(kind: u8) -> Option<Self> {
        if kind > MAX_PARAMETER_KIND {
            return None;
        }
        Some(Self { kind })
    }

    /// The kind of parameter, 0..=63.
    pub const fn kind(self) -> u8 {
        self.kind
    }

    /// Decodes the first octet of `src`.
    pub fn decode(src: &[u8]) -> Option<Self> {
        Self::new(field_u8(src, 0, 6)?)
    }

    /// Writes the octet into the front of `dst`.
    pub fn encode(self, dst: &mut [u8]) -> Option<()> {
        begin(dst, Self::SIZE)?;
        put(dst, 0, 6, u64::from(self.kind))
    }
}
