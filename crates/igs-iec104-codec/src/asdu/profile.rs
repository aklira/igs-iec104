// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! The rules of the 104 application profile that a decoded ASDU must satisfy.
//!
//! The causes of transmission come from the generated profile, which already
//! applies decisions D-001 to D-005 (PROVENANCE.md). The direction of each
//! type comes from the tables of IEC 60870-5-104 9.5.

use std::fmt;

use super::Asdu;
use crate::generated::profile::TypeId;
use crate::header::CauseOfTransmission;

/// The direction in which an ASDU is sent.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Direction {
    /// From the controlled station to the controlling station.
    Monitor,
    /// From the controlling station to the controlled station.
    Control,
}

/// The profile rule that a type, a sequence form or a cause broke.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ProfileError {
    /// The type identification is not in the 104 profile.
    TypeNotInProfile(TypeId),
    /// The type is not sent in this direction (104 9.5).
    WrongDirection {
        /// The type identification.
        type_id: TypeId,
        /// The direction it was sent in.
        direction: Direction,
    },
    /// The SQ = 1 form is not permitted for this type.
    SequenceNotPermitted(TypeId),
    /// The cause is marked not permitted for this type.
    CauseNotPermitted {
        /// The type identification.
        type_id: TypeId,
        /// The cause of transmission.
        cause: u8,
    },
    /// The cause is not in the list the profile allows for this type.
    CauseNotAllowed {
        /// The type identification.
        type_id: TypeId,
        /// The cause of transmission.
        cause: u8,
    },
}

impl fmt::Display for ProfileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TypeNotInProfile(type_id) => {
                write!(f, "{} is not in the 104 profile", type_id.mnemonic())
            }
            Self::WrongDirection { type_id, direction } => {
                write!(
                    f,
                    "{} is not sent in the {direction:?} direction",
                    type_id.mnemonic()
                )
            }
            Self::SequenceNotPermitted(type_id) => {
                write!(f, "{} does not permit SQ = 1", type_id.mnemonic())
            }
            Self::CauseNotPermitted { type_id, cause } => {
                write!(
                    f,
                    "cause {cause} is not permitted for {}",
                    type_id.mnemonic()
                )
            }
            Self::CauseNotAllowed { type_id, cause } => {
                write!(f, "cause {cause} is not allowed for {}", type_id.mnemonic())
            }
        }
    }
}

impl std::error::Error for ProfileError {}

/// Whether a type identification is sent in `direction` (104 9.5). The
/// monitor-direction types are 1 to 44 and 70; the control-direction types are
/// 45 to 69, 100 to 107 and 110 to 113; the file transfer types (120 to 127)
/// are used in both directions.
fn sent_in(code: u8, direction: Direction) -> bool {
    match code {
        1..=44 | 70 => direction == Direction::Monitor,
        45..=69 | 100..=107 | 110..=113 => direction == Direction::Control,
        120..=127 => true,
        _ => false,
    }
}

/// Checks the type identification alone: it must be in the profile and sent in
/// `direction`. Use it on a type identification read from a header.
pub fn validate_type(type_id: TypeId, direction: Direction) -> Result<(), ProfileError> {
    if !type_id.in_profile() {
        return Err(ProfileError::TypeNotInProfile(type_id));
    }
    if !sent_in(type_id.as_u8(), direction) {
        return Err(ProfileError::WrongDirection { type_id, direction });
    }
    Ok(())
}

/// Checks a decoded ASDU against the profile: its type, its direction, its
/// sequence form (SQ) and its cause of transmission. Returns the first rule
/// that fails.
pub fn validate_profile(asdu: &Asdu, direction: Direction) -> Result<(), ProfileError> {
    let type_id = asdu.type_id();
    validate_type(type_id, direction)?;
    validate_sequence(type_id, asdu.body.is_sequence())?;
    validate_cause(type_id, asdu.cot)
}

fn validate_sequence(type_id: TypeId, sequence: bool) -> Result<(), ProfileError> {
    // Bit 1 of the mask is SQ = 1 (generated profile).
    let sequence_bit = 0b10;
    if sequence && type_id.sq_allowed_mask() & sequence_bit == 0 {
        return Err(ProfileError::SequenceNotPermitted(type_id));
    }
    Ok(())
}

fn validate_cause(type_id: TypeId, cot: CauseOfTransmission) -> Result<(), ProfileError> {
    let cause = cot.cause();
    if type_id.cot_not_permitted().contains(&cause) {
        return Err(ProfileError::CauseNotPermitted { type_id, cause });
    }
    if !type_id.cot_allowed().contains(&cause) {
        return Err(ProfileError::CauseNotAllowed { type_id, cause });
    }
    Ok(())
}
