// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! Application service data units of the 104 profile: decoding and encoding
//! of a whole ASDU (data unit identifier and information objects), and the
//! profile rules that a decoded ASDU must satisfy.
//!
//! [`Asdu::decode`](crate::asdu::Asdu::decode) accepts every in-profile type ID and rejects the
//! others with [`DecodeError::UnsupportedTypeId`](crate::error::DecodeError::UnsupportedTypeId),
//! which carries the raw octets. [`validate_profile`](crate::asdu::validate_profile) checks the
//! rules of the profile (type, sequence form, cause of transmission, direction) and says which one
//! failed.

mod body;
mod objects;
mod profile;
mod wire;

#[cfg(test)]
mod tests;

use crate::apci::CONTROL_FIELD_OCTETS;
use crate::error::{DecodeError, EncodeError};
use crate::generated::profile::{TypeId, CAUSE_OF_TRANSMISSION_OCTS, MAX_APDU_LENGTH};
use crate::header::{AsduHeader, CauseOfTransmission, CommonAddress, VariableStructure};

pub use body::Body;
pub use objects::{InformationObject, Objects, MAX_OBJECTS};
pub use profile::{validate_profile, validate_type, Direction, ProfileError};
pub use wire::{FileSegment, Timed, Wire};

/// Largest ASDU, in octets. The APDU length field is at most 253 and counts the
/// four control field octets, so the ASDU is at most 249 octets (104 §5 and
/// §9.5; see PROVENANCE.md D-010).
pub const MAX_ASDU_LENGTH: usize = match MAX_APDU_LENGTH.checked_sub(CONTROL_FIELD_OCTETS) {
    Some(length) => length,
    None => 0,
};

/// Octets of the largest ASDU that can be built: the header and 127 objects of
/// at most 262 octets (address, then a file segment of 255 octets and its 4 octets).
const LARGEST_BUILDABLE_ASDU: usize = 6 + 127 * 262;

// The header is six octets: the type identification, the VSQ, the two-octet
// cause and the two-octet common address.
const _: () = assert!(CAUSE_OF_TRANSMISSION_OCTS == 2);

/// One ASDU: its cause, its common address and its information objects. The
/// type identification is the variant of `body`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Asdu {
    /// Cause of transmission, with the P/N, test and originator fields.
    pub cot: CauseOfTransmission,
    /// Common address of ASDU.
    pub common_address: CommonAddress,
    /// The information objects, of one type identification.
    pub body: Body,
}

impl Asdu {
    /// The type identification of the ASDU.
    pub fn type_id(&self) -> TypeId {
        self.body.type_id()
    }

    /// Decodes one ASDU from `src`, which holds exactly the ASDU octets.
    ///
    /// Fails when the ASDU is longer than [`MAX_ASDU_LENGTH`], when the header
    /// is truncated or unknown, when the type is outside the profile (the error
    /// carries the octets), when the objects do not fill the ASDU, or when an
    /// element value is out of range.
    pub fn decode(src: &[u8]) -> Result<Self, DecodeError> {
        if src.len() > MAX_ASDU_LENGTH {
            return Err(DecodeError::TooLong {
                length: src.len(),
                max: MAX_ASDU_LENGTH,
            });
        }
        let header = AsduHeader::decode(src)?;
        if !header.type_id.in_profile() {
            return Err(DecodeError::UnsupportedTypeId {
                type_code: header.type_id.as_u8(),
                raw: src.to_vec(),
            });
        }
        let body_octets = src.get(AsduHeader::SIZE..).ok_or(DecodeError::Truncated {
            needed: AsduHeader::SIZE,
            available: src.len(),
        })?;
        let body = Body::decode(
            header.type_id,
            body_octets,
            header.vsq.number(),
            header.vsq.sequence,
        )?;
        Ok(Self {
            cot: header.cot,
            common_address: header.common_address,
            body,
        })
    }

    /// Writes the ASDU into the front of `dst`. Returns the octets written.
    ///
    /// Fails when the objects do not fit in [`MAX_ASDU_LENGTH`] or in `dst`, or
    /// when there are more than 127 of them.
    pub fn encode(&self, dst: &mut [u8]) -> Result<usize, EncodeError> {
        let number = u8::try_from(self.body.len()).map_err(|_| EncodeError::TooManyObjects {
            count: self.body.len(),
        })?;
        let variable = VariableStructure::new(number, self.body.is_sequence()).ok_or(
            EncodeError::TooManyObjects {
                count: self.body.len(),
            },
        )?;
        let header = AsduHeader {
            type_id: self.body.type_id(),
            vsq: variable,
            cot: self.cot,
            common_address: self.common_address,
        };
        let available = dst.len();
        let header_window = dst
            .get_mut(..AsduHeader::SIZE)
            .ok_or(EncodeError::BufferTooSmall {
                needed: AsduHeader::SIZE,
                available,
            })?;
        header.encode(header_window)?;
        let body_window = dst
            .get_mut(AsduHeader::SIZE..)
            .ok_or(EncodeError::BufferTooSmall {
                needed: AsduHeader::SIZE,
                available,
            })?;
        let body_length = self.body.encode(body_window)?;
        let length = AsduHeader::SIZE.saturating_add(body_length);
        if length > MAX_ASDU_LENGTH {
            return Err(EncodeError::TooLong {
                length,
                max: MAX_ASDU_LENGTH,
            });
        }
        Ok(length)
    }

    /// The ASDU as a new vector of octets. The buffer holds the largest ASDU
    /// that can be built (127 objects of at most 262 octets, and the header), so
    /// that an oversized ASDU is reported as [`EncodeError::TooLong`].
    pub fn to_vec(&self) -> Result<Vec<u8>, EncodeError> {
        let mut dst = vec![0; LARGEST_BUILDABLE_ASDU];
        let length = self.encode(&mut dst)?;
        dst.truncate(length);
        Ok(dst)
    }
}
