// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! ASDU header and information object address (IEC 60870-5-101 7.2.1 to 7.2.5,
//! and the 104 field sizes of IEC 60870-5-104 9.5).
//!
//! The data unit identifier of an ASDU is six octets: the type identification
//! (1), the variable structure qualifier (1), the cause of transmission with
//! its originator address (2) and the common address (2). Each information
//! object then starts with its address (3 octets). Multi-octet fields are
//! little-endian, as 104 9.5 requires.

use crate::error::{DecodeError, EncodeError};
use crate::formats::{get_bits, set_bits};
use crate::generated::profile::{
    TypeId, CAUSE_OF_TRANSMISSION_OCTS, COMMON_ADDRESS_OCTS, INFO_OBJECT_ADDRESS_OCTS,
};

// The layout below is fixed in code: the profile must agree with it.
const _: () = assert!(CAUSE_OF_TRANSMISSION_OCTS == 2);
const _: () = assert!(COMMON_ADDRESS_OCTS == 2);
const _: () = assert!(INFO_OBJECT_ADDRESS_OCTS == 3);

const MAX_NUMBER: u8 = 127;
const MAX_CAUSE: u8 = 63;
const MAX_INFO_OBJECT_ADDRESS: u32 = 0x00FF_FFFF;

/// Cause of transmission values of IEC 60870-5-101 Table 14 (data only).
pub mod cause {
    /// Not used.
    pub const NOT_USED: u8 = 0;
    /// Periodic, cyclic.
    pub const PERIODIC: u8 = 1;
    /// Background scan.
    pub const BACKGROUND_SCAN: u8 = 2;
    /// Spontaneous.
    pub const SPONTANEOUS: u8 = 3;
    /// Initialized.
    pub const INITIALIZED: u8 = 4;
    /// Request or requested.
    pub const REQUEST: u8 = 5;
    /// Activation.
    pub const ACTIVATION: u8 = 6;
    /// Activation confirmation.
    pub const ACTIVATION_CONFIRMATION: u8 = 7;
    /// Deactivation.
    pub const DEACTIVATION: u8 = 8;
    /// Deactivation confirmation.
    pub const DEACTIVATION_CONFIRMATION: u8 = 9;
    /// Activation termination.
    pub const ACTIVATION_TERMINATION: u8 = 10;
    /// Return information caused by a remote command.
    pub const RETURN_REMOTE: u8 = 11;
    /// Return information caused by a local command.
    pub const RETURN_LOCAL: u8 = 12;
    /// File transfer.
    pub const FILE_TRANSFER: u8 = 13;
    /// Interrogated by station interrogation.
    pub const INTERROGATED_STATION: u8 = 20;
    /// Interrogated by group 1 interrogation (groups 1 to 16 are 21 to 36).
    pub const INTERROGATED_GROUP_1: u8 = 21;
    /// Requested by general counter request.
    pub const COUNTER_GENERAL: u8 = 37;
    /// Requested by group 1 counter request (groups 1 to 4 are 38 to 41).
    pub const COUNTER_GROUP_1: u8 = 38;
    /// Unknown type identification.
    pub const UNKNOWN_TYPE_ID: u8 = 44;
    /// Unknown cause of transmission.
    pub const UNKNOWN_CAUSE: u8 = 45;
    /// Unknown common address of ASDU.
    pub const UNKNOWN_COMMON_ADDRESS: u8 = 46;
    /// Unknown information object address.
    pub const UNKNOWN_INFO_OBJECT_ADDRESS: u8 = 47;
}

/// Variable structure qualifier (101 7.2.2): the number of information objects
/// (or of element combinations) and whether they form a sequence.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct VariableStructure {
    number: u8,
    /// SQ: one information object with consecutive addresses (true), or
    /// several objects each with its own address (false).
    pub sequence: bool,
}

impl VariableStructure {
    /// Fails when `number` is above 127 (seven bits).
    pub const fn new(number: u8, sequence: bool) -> Option<Self> {
        if number > MAX_NUMBER {
            return None;
        }
        Some(Self { number, sequence })
    }

    /// The number of objects or element combinations, 0..=127.
    pub const fn number(self) -> u8 {
        self.number
    }
}

/// Cause of transmission with the P/N and test bits and the originator
/// address (101 7.2.3).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct CauseOfTransmission {
    cause: u8,
    /// P/N: negative confirmation.
    pub negative: bool,
    /// T: the ASDU was generated under test conditions.
    pub test: bool,
    /// Originator address; 0 when not used.
    pub originator: u8,
}

impl CauseOfTransmission {
    /// Fails when `cause` is above 63 (six bits). The flags start cleared.
    pub const fn new(cause: u8) -> Option<Self> {
        if cause > MAX_CAUSE {
            return None;
        }
        Some(Self {
            cause,
            negative: false,
            test: false,
            originator: 0,
        })
    }

    /// The cause, 0..=63; see the [`cause`] module.
    pub const fn cause(self) -> u8 {
        self.cause
    }
}

/// Common address of ASDU (101 7.2.4): a station address, or the global
/// address `GLOBAL` (broadcast).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct CommonAddress(u16);

impl CommonAddress {
    /// The broadcast address of the two-octet common address.
    pub const GLOBAL: Self = Self(0xFFFF);

    /// Wraps an address. Every `u16` is accepted here; 0 (not used) is
    /// rejected by the profile validation, not by the codec.
    pub const fn new(address: u16) -> Self {
        Self(address)
    }

    /// The address, 0..=65535.
    pub const fn value(self) -> u16 {
        self.0
    }

    /// True for the global (broadcast) address.
    pub const fn is_global(self) -> bool {
        self.0 == Self::GLOBAL.0
    }
}

/// Information object address (101 7.2.5), three octets. 0 means "not
/// relevant".
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct InformationObjectAddress(u32);

impl InformationObjectAddress {
    /// Octets on the wire.
    pub const SIZE: usize = 3;

    /// Fails when `address` does not fit in 24 bits.
    pub const fn new(address: u32) -> Option<Self> {
        if address > MAX_INFO_OBJECT_ADDRESS {
            return None;
        }
        Some(Self(address))
    }

    /// The address, 0..=16 777 215.
    pub const fn value(self) -> u32 {
        self.0
    }

    /// Decodes the three octets at the front of `src`.
    pub fn decode(src: &[u8]) -> Result<Self, DecodeError> {
        let raw = read(src, 0, 24, Self::SIZE)?;
        let address = u32::try_from(raw).map_err(|_| truncated(Self::SIZE, src))?;
        Self::new(address).ok_or_else(|| truncated(Self::SIZE, src))
    }

    /// Writes the three octets into the front of `dst`.
    pub fn encode(self, dst: &mut [u8]) -> Result<(), EncodeError> {
        clear(dst, Self::SIZE)?;
        write(dst, 0, 24, u64::from(self.0), Self::SIZE)
    }
}

/// The data unit identifier of an ASDU (101 7.2.1 to 7.2.4).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct AsduHeader {
    /// Type identification (101 7.2.1).
    pub type_id: TypeId,
    /// Variable structure qualifier (101 7.2.2).
    pub vsq: VariableStructure,
    /// Cause of transmission (101 7.2.3).
    pub cot: CauseOfTransmission,
    /// Common address of ASDU (101 7.2.4).
    pub common_address: CommonAddress,
}

impl AsduHeader {
    /// Octets on the wire: type identification, VSQ, cause of transmission
    /// (with originator address) and common address.
    pub const SIZE: usize = 6;

    /// Decodes the header at the front of `src`. Fails on truncated input and
    /// on a type identification outside the catalogue.
    pub fn decode(src: &[u8]) -> Result<Self, DecodeError> {
        let code =
            u8::try_from(read(src, 0, 8, Self::SIZE)?).map_err(|_| truncated(Self::SIZE, src))?;
        let type_id = TypeId::from_u8(code).ok_or(DecodeError::UnknownTypeId(code))?;

        let number =
            u8::try_from(read(src, 8, 7, Self::SIZE)?).map_err(|_| truncated(Self::SIZE, src))?;
        let sequence = read(src, 15, 1, Self::SIZE)? == 1;

        let cause =
            u8::try_from(read(src, 16, 6, Self::SIZE)?).map_err(|_| truncated(Self::SIZE, src))?;
        let negative = read(src, 22, 1, Self::SIZE)? == 1;
        let test = read(src, 23, 1, Self::SIZE)? == 1;
        let originator =
            u8::try_from(read(src, 24, 8, Self::SIZE)?).map_err(|_| truncated(Self::SIZE, src))?;

        let common = u16::try_from(read(src, 32, 16, Self::SIZE)?)
            .map_err(|_| truncated(Self::SIZE, src))?;

        Ok(Self {
            type_id,
            vsq: VariableStructure { number, sequence },
            cot: CauseOfTransmission {
                cause,
                negative,
                test,
                originator,
            },
            common_address: CommonAddress::new(common),
        })
    }

    /// Writes the header into the front of `dst`.
    pub fn encode(self, dst: &mut [u8]) -> Result<(), EncodeError> {
        clear(dst, Self::SIZE)?;
        write(dst, 0, 8, u64::from(self.type_id.as_u8()), Self::SIZE)?;
        write(dst, 8, 7, u64::from(self.vsq.number), Self::SIZE)?;
        write(dst, 15, 1, u64::from(self.vsq.sequence), Self::SIZE)?;
        write(dst, 16, 6, u64::from(self.cot.cause), Self::SIZE)?;
        write(dst, 22, 1, u64::from(self.cot.negative), Self::SIZE)?;
        write(dst, 23, 1, u64::from(self.cot.test), Self::SIZE)?;
        write(dst, 24, 8, u64::from(self.cot.originator), Self::SIZE)?;
        write(dst, 32, 16, u64::from(self.common_address.0), Self::SIZE)
    }
}

/// Reads `width` bits at zero-based index `start`. Fails when `src` is too
/// short for the whole header (`needed` octets).
fn read(src: &[u8], start: usize, width: usize, needed: usize) -> Result<u64, DecodeError> {
    if src.len() < needed {
        return Err(truncated(needed, src));
    }
    get_bits(src, start, width).ok_or_else(|| truncated(needed, src))
}

fn truncated(needed: usize, src: &[u8]) -> DecodeError {
    DecodeError::Truncated {
        needed,
        available: src.len(),
    }
}

/// Zeroes the first `size` octets, so that every bit is written by the caller.
fn clear(dst: &mut [u8], size: usize) -> Result<(), EncodeError> {
    let available = dst.len();
    let octets = dst.get_mut(..size).ok_or(EncodeError::BufferTooSmall {
        needed: size,
        available,
    })?;
    octets.fill(0);
    Ok(())
}

fn write(
    dst: &mut [u8],
    start: usize,
    width: usize,
    value: u64,
    needed: usize,
) -> Result<(), EncodeError> {
    set_bits(dst, start, width, value).ok_or(EncodeError::BufferTooSmall {
        needed,
        available: dst.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generated::profile::{COMMON_ADDRESS_OCTS, INFO_OBJECT_ADDRESS_OCTS};

    fn header(type_id: TypeId, number: u8, sequence: bool, cause: u8, ca: u16) -> AsduHeader {
        AsduHeader {
            type_id,
            vsq: VariableStructure::new(number, sequence).expect("in range"),
            cot: CauseOfTransmission::new(cause).expect("in range"),
            common_address: CommonAddress::new(ca),
        }
    }

    #[test]
    fn single_command_hand_written_vector() {
        // C_IC_NA_1 (100), one object, activation (6), common address 1.
        let value = header(TypeId::C_IC_NA_1, 1, false, cause::ACTIVATION, 1);
        let mut dst = [0xFFu8; AsduHeader::SIZE];
        value.encode(&mut dst).expect("six octets");
        assert_eq!(dst, [0x64, 0x01, 0x06, 0x00, 0x01, 0x00]);
        assert_eq!(AsduHeader::decode(&dst), Ok(value));
    }

    #[test]
    fn sequence_with_flags_and_originator_hand_written_vector() {
        // M_ME_NB_1 (11), two elements in a sequence, spontaneous (3) with
        // negative confirmation (P/N = bit 7) and test (T = bit 8), originator 7,
        // common address 0x1234 (little-endian on the wire).
        let mut value = header(TypeId::M_ME_NB_1, 2, true, cause::SPONTANEOUS, 0x1234);
        value.cot.negative = true;
        value.cot.test = true;
        value.cot.originator = 7;
        let mut dst = [0xFFu8; AsduHeader::SIZE];
        value.encode(&mut dst).expect("six octets");
        assert_eq!(dst, [0x0B, 0x82, 0xC3, 0x07, 0x34, 0x12]);
        assert_eq!(AsduHeader::decode(&dst), Ok(value));
    }

    #[test]
    fn every_type_identification_round_trips() {
        for type_id in TypeId::ALL {
            let value = header(type_id, 127, true, cause::REQUEST, 0xFFFF);
            let mut dst = [0u8; AsduHeader::SIZE];
            value.encode(&mut dst).expect("six octets");
            assert_eq!(
                AsduHeader::decode(&dst),
                Ok(value),
                "{}",
                type_id.mnemonic()
            );
        }
    }

    #[test]
    fn decoding_rejects_truncated_input() {
        let full = [0x64, 0x01, 0x06, 0x00, 0x01, 0x00];
        for len in 0..AsduHeader::SIZE {
            assert_eq!(
                AsduHeader::decode(&full[..len]),
                Err(DecodeError::Truncated {
                    needed: AsduHeader::SIZE,
                    available: len,
                }),
                "{len} octets"
            );
        }
    }

    #[test]
    fn decoding_rejects_unknown_type_identification() {
        // 0 and 200 are not in the catalogue of 67 type IDs.
        assert_eq!(
            AsduHeader::decode(&[0x00, 0x01, 0x06, 0x00, 0x01, 0x00]),
            Err(DecodeError::UnknownTypeId(0))
        );
        assert_eq!(
            AsduHeader::decode(&[200, 0x01, 0x06, 0x00, 0x01, 0x00]),
            Err(DecodeError::UnknownTypeId(200))
        );
    }

    #[test]
    fn decoding_accepts_types_outside_the_profile() {
        // Profile validation is a later step (C4); the header only checks the catalogue.
        let decoded = AsduHeader::decode(&[0x02, 0x01, 0x03, 0x00, 0x01, 0x00]).expect("catalogue");
        assert_eq!(decoded.type_id, TypeId::M_SP_TA_1);
        assert!(!decoded.type_id.in_profile());
    }

    #[test]
    fn encoding_rejects_short_buffers() {
        let value = header(TypeId::C_IC_NA_1, 1, false, cause::ACTIVATION, 1);
        assert_eq!(
            value.encode(&mut [0u8; 5]),
            Err(EncodeError::BufferTooSmall {
                needed: 6,
                available: 5
            })
        );
    }

    #[test]
    fn field_ranges() {
        assert!(VariableStructure::new(127, true).is_some());
        assert!(VariableStructure::new(128, false).is_none());
        assert!(CauseOfTransmission::new(63).is_some());
        assert!(CauseOfTransmission::new(64).is_none());
        assert!(CommonAddress::GLOBAL.is_global());
        assert_eq!(CommonAddress::new(0xFFFF).value(), 0xFFFF);
    }

    #[test]
    fn cause_values_are_the_101_table() {
        assert_eq!(cause::SPONTANEOUS, 3);
        assert_eq!(cause::ACTIVATION, 6);
        assert_eq!(cause::ACTIVATION_TERMINATION, 10);
        assert_eq!(cause::UNKNOWN_INFO_OBJECT_ADDRESS, 47);
    }

    #[test]
    fn information_object_address_round_trip_and_truncation() {
        let address = InformationObjectAddress::new(0x0012_3456).expect("24 bits");
        let mut dst = [0xFFu8; 3];
        address.encode(&mut dst).expect("three octets");
        assert_eq!(dst, [0x56, 0x34, 0x12]);
        assert_eq!(InformationObjectAddress::decode(&dst), Ok(address));
        assert_eq!(InformationObjectAddress::new(0x0100_0000), None);
        assert_eq!(
            InformationObjectAddress::decode(&[0x01, 0x02]),
            Err(DecodeError::Truncated {
                needed: 3,
                available: 2
            })
        );
        assert_eq!(
            InformationObjectAddress::new(1)
                .expect("24 bits")
                .encode(&mut [0u8; 2]),
            Err(EncodeError::BufferTooSmall {
                needed: 3,
                available: 2
            })
        );
    }

    #[test]
    fn sizes_match_the_generated_profile() {
        // Header: type (1) + VSQ (1) + cause (2) + common address (2).
        assert_eq!(
            AsduHeader::SIZE,
            1 + 1 + usize::from(CAUSE_OF_TRANSMISSION_OCTS) + usize::from(COMMON_ADDRESS_OCTS)
        );
        assert_eq!(
            InformationObjectAddress::SIZE,
            usize::from(INFO_OBJECT_ADDRESS_OCTS)
        );
    }
}
