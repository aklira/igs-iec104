// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! ASDU tests: one vector per in-profile type ID, round trips of both
//! addressing forms, the decoding and encoding errors, and the profile rules
//! (D-001 to D-005 among them).
//!
//! The vectors are built by hand: the header and the address are written
//! octet by octet, and each element value is a sample written in the octets
//! of its clause. The length of the value is checked against the generated
//! object size.

use super::*;
use crate::elements::{Nof, Nos, Nva, Qds, Siq};
use crate::formats::Cp56Time2a;
use crate::generated::profile::TypeId;
use crate::header::{cause, CauseOfTransmission, CommonAddress, InformationObjectAddress};

// Sample octets of the elements (values are examples; only the layout matters).
const SIQ: &[u8] = &[0x11];
const DIQ: &[u8] = &[0x02];
const QDS: &[u8] = &[0x00];
const VTI: &[u8] = &[0x3F];
const BSI: &[u8] = &[0x01, 0x00, 0x00, 0x80];
const NVA: &[u8] = &[0x00, 0x40];
const SVA: &[u8] = &[0x10, 0x27];
const R32: &[u8] = &[0x00, 0x00, 0x80, 0x3F];
const BCR: &[u8] = &[0x01, 0x00, 0x00, 0x00, 0x05];
const SCD: &[u8] = &[0x01, 0x00, 0x00, 0x80];
const SEP: &[u8] = &[0x02];
const SPE: &[u8] = &[0x01];
const QDP: &[u8] = &[0x00];
const OCI: &[u8] = &[0x01];
const CP16: &[u8] = &[0xE8, 0x03];
const SCO: &[u8] = &[0x81];
const DCO: &[u8] = &[0x02];
const RCO: &[u8] = &[0x01];
const QOS: &[u8] = &[0x80];
const COI: &[u8] = &[0x00];
const QOI: &[u8] = &[0x14];
const QCC: &[u8] = &[0x05];
const QRP: &[u8] = &[0x01];
const TSC: &[u8] = &[0x34, 0x12];
const QPM: &[u8] = &[0x01];
const QPA: &[u8] = &[0x01];
const NOF: &[u8] = &[0x01, 0x00];
const NOS: &[u8] = &[0x00];
const LOF: &[u8] = &[0x03, 0x00, 0x00];
const FRQ: &[u8] = &[0x00];
const SRQ: &[u8] = &[0x00];
const SCQ: &[u8] = &[0x00];
const LSQ: &[u8] = &[0x01];
const CHS: &[u8] = &[0x00];
const AFQ: &[u8] = &[0x00];
const SOF: &[u8] = &[0x00];
const LOS: &[u8] = &[0x02];
const SEGMENT_DATA: &[u8] = &[0xAA, 0x55];
const ZERO_TIME: &[u8] = &[0x00; 7];
const BAD_TIME: &[u8] = &[0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00];
const CP56: &[u8] = &[0xE8, 0x03, 0x1E, 0x0C, 0x6F, 0x07, 0x18];

/// Sample value octets of a type ID, element by element, time tag last.
fn body_octets(type_id: TypeId) -> Vec<u8> {
    match type_id {
        TypeId::M_SP_NA_1 => [SIQ].concat(),
        TypeId::M_DP_NA_1 => [DIQ].concat(),
        TypeId::M_ST_NA_1 => [VTI, QDS].concat(),
        TypeId::M_BO_NA_1 => [BSI, QDS].concat(),
        TypeId::M_ME_NA_1 => [NVA, QDS].concat(),
        TypeId::M_ME_NB_1 => [SVA, QDS].concat(),
        TypeId::M_ME_NC_1 => [R32, QDS].concat(),
        TypeId::M_IT_NA_1 => [BCR].concat(),
        TypeId::M_PS_NA_1 => [SCD, QDS].concat(),
        TypeId::M_ME_ND_1 => [NVA].concat(),
        TypeId::M_SP_TB_1 => [SIQ, CP56].concat(),
        TypeId::M_DP_TB_1 => [DIQ, CP56].concat(),
        TypeId::M_ST_TB_1 => [VTI, QDS, CP56].concat(),
        TypeId::M_BO_TB_1 => [BSI, QDS, CP56].concat(),
        TypeId::M_ME_TD_1 => [NVA, QDS, CP56].concat(),
        TypeId::M_ME_TE_1 => [SVA, QDS, CP56].concat(),
        TypeId::M_ME_TF_1 => [R32, QDS, CP56].concat(),
        TypeId::M_IT_TB_1 => [BCR, CP56].concat(),
        TypeId::M_EP_TD_1 => [SEP, CP16, CP56].concat(),
        TypeId::M_EP_TE_1 => [SPE, QDP, CP16, CP56].concat(),
        TypeId::M_EP_TF_1 => [OCI, QDP, CP16, CP56].concat(),
        TypeId::C_SC_NA_1 => [SCO].concat(),
        TypeId::C_DC_NA_1 => [DCO].concat(),
        TypeId::C_RC_NA_1 => [RCO].concat(),
        TypeId::C_SE_NA_1 => [NVA, QOS].concat(),
        TypeId::C_SE_NB_1 => [SVA, QOS].concat(),
        TypeId::C_SE_NC_1 => [R32, QOS].concat(),
        TypeId::C_BO_NA_1 => [BSI].concat(),
        TypeId::C_SC_TA_1 => [SCO, CP56].concat(),
        TypeId::C_DC_TA_1 => [DCO, CP56].concat(),
        TypeId::C_RC_TA_1 => [RCO, CP56].concat(),
        TypeId::C_SE_TA_1 => [NVA, QOS, CP56].concat(),
        TypeId::C_SE_TB_1 => [SVA, QOS, CP56].concat(),
        TypeId::C_SE_TC_1 => [R32, QOS, CP56].concat(),
        TypeId::C_BO_TA_1 => [BSI, CP56].concat(),
        TypeId::M_EI_NA_1 => [COI].concat(),
        TypeId::C_IC_NA_1 => [QOI].concat(),
        TypeId::C_CI_NA_1 => [QCC].concat(),
        TypeId::C_RD_NA_1 => Vec::new(),
        TypeId::C_CS_NA_1 => [CP56].concat(),
        TypeId::C_RP_NA_1 => [QRP].concat(),
        TypeId::C_TS_TA_1 => [TSC, CP56].concat(),
        TypeId::P_ME_NA_1 => [NVA, QPM].concat(),
        TypeId::P_ME_NB_1 => [SVA, QPM].concat(),
        TypeId::P_ME_NC_1 => [R32, QPM].concat(),
        TypeId::P_AC_NA_1 => [QPA].concat(),
        TypeId::F_FR_NA_1 => [NOF, LOF, FRQ].concat(),
        TypeId::F_SR_NA_1 => [NOF, NOS, LOF, SRQ].concat(),
        TypeId::F_SC_NA_1 => [NOF, NOS, SCQ].concat(),
        TypeId::F_LS_NA_1 => [NOF, NOS, LSQ, CHS].concat(),
        TypeId::F_AF_NA_1 => [NOF, NOS, AFQ].concat(),
        TypeId::F_SG_NA_1 => [NOF, NOS, LOS, SEGMENT_DATA].concat(),
        TypeId::F_DR_TA_1 => [NOF, LOF, SOF, CP56].concat(),
        TypeId::F_SC_NB_1 => [NOF, ZERO_TIME, CP56].concat(),
        _ => Vec::new(),
    }
}

/// The direction in which a type is sent (104 9.5), for the vectors.
fn direction_of(type_id: TypeId) -> Direction {
    match type_id.as_u8() {
        45..=69 | 100..=107 | 110..=113 => Direction::Control,
        _ => Direction::Monitor,
    }
}

/// Octets of one ASDU: the hand-written header, the common address 1 and
/// the given body. `number` counts the objects; `sequence` selects SQ = 1.
fn asdu_octets(
    type_id: TypeId,
    number: u8,
    sequence: bool,
    cause_code: u8,
    body: &[u8],
) -> Vec<u8> {
    let vsq = if sequence { 0x80 } else { 0x00 } | number;
    let mut octets = vec![type_id.as_u8(), vsq, cause_code, 0x00, 0x01, 0x00];
    octets.extend_from_slice(body);
    octets
}

/// The information object address 1, three octets.
const ADDRESS_ONE: &[u8] = &[0x01, 0x00, 0x00];

/// An ASDU with one individual object. `body` holds the object: its address
/// and its value.
fn one_object(type_id: TypeId, cause_code: u8, body: &[u8]) -> Vec<u8> {
    asdu_octets(type_id, 1, false, cause_code, body)
}

#[test]
fn every_in_profile_type_has_a_vector_that_round_trips() {
    for type_id in TypeId::ALL.iter().copied().filter(|t| t.in_profile()) {
        let direction = direction_of(type_id);
        let cause_code = type_id.cot_allowed()[0];
        let body = body_octets(type_id);

        // The value is as long as the generated object size, when that size is fixed.
        if let Some(bits) = type_id.object_size_bits() {
            assert_eq!(
                body.len(),
                usize::from(bits) / 8,
                "value of {} against the generated size",
                type_id.mnemonic()
            );
        }

        let mut object = ADDRESS_ONE.to_vec();
        object.extend_from_slice(&body);
        // F_DR_TA_1 is only permitted in the SQ = 1 form (one address, one set).
        let bytes = if type_id.sq_allowed_mask() == 0b10 {
            asdu_octets(type_id, 1, true, cause_code, &object)
        } else {
            one_object(type_id, cause_code, &object)
        };

        let asdu = Asdu::decode(&bytes)
            .unwrap_or_else(|e| panic!("{} does not decode: {e}", type_id.mnemonic()));
        assert_eq!(asdu.type_id(), type_id);
        assert_eq!(asdu.body.len(), 1);
        assert_eq!(
            validate_profile(&asdu, direction),
            Ok(()),
            "{} with cause {cause_code}",
            type_id.mnemonic()
        );
        assert_eq!(
            asdu.to_vec(),
            Ok(bytes),
            "round trip of {}",
            type_id.mnemonic()
        );
    }
}

/// Decodes the bytes of an ASDU and runs the profile rules on it.
fn check(bytes: &[u8], direction: Direction) -> Result<(), ProfileError> {
    validate_profile(&Asdu::decode(bytes).expect("decodes"), direction)
}

#[test]
fn d001_no_return_information_for_packed_single_point() {
    // M_PS_NA_1 (TI 20) is not sent with the return-information causes 11 and 12.
    let body = [ADDRESS_ONE, SCD, QDS].concat();
    for cause_code in [cause::RETURN_REMOTE, cause::RETURN_LOCAL] {
        assert_eq!(
            check(
                &asdu_octets(TypeId::M_PS_NA_1, 1, false, cause_code, &body),
                Direction::Monitor
            ),
            Err(ProfileError::CauseNotAllowed {
                type_id: TypeId::M_PS_NA_1,
                cause: cause_code,
            })
        );
    }
    assert_eq!(
        check(
            &asdu_octets(TypeId::M_PS_NA_1, 1, false, cause::SPONTANEOUS, &body),
            Direction::Monitor
        ),
        Ok(())
    );
}

#[test]
fn d002_deactivation_of_bitstring_command_only_for_the_untimed_type() {
    // C_BO_NA_1 (TI 51) accepts deactivation (8) and its confirmation (9);
    // C_BO_TA_1 (TI 64) does not accept deactivation.
    for cause_code in [cause::DEACTIVATION, cause::DEACTIVATION_CONFIRMATION] {
        let untimed = one_object(TypeId::C_BO_NA_1, cause_code, &[ADDRESS_ONE, BSI].concat());
        assert_eq!(
            check(&untimed, Direction::Control),
            Ok(()),
            "cause {cause_code}"
        );
    }
    let timed = one_object(
        TypeId::C_BO_TA_1,
        cause::DEACTIVATION,
        &[ADDRESS_ONE, BSI, CP56].concat(),
    );
    assert_eq!(
        check(&timed, Direction::Control),
        Err(ProfileError::CauseNotAllowed {
            type_id: TypeId::C_BO_TA_1,
            cause: cause::DEACTIVATION,
        })
    );
}

#[test]
fn d003_no_deactivation_for_integrated_totals_request() {
    // C_CI_NA_1 (TI 101) is not sent with deactivation (8) or its confirmation (9).
    for cause_code in [cause::DEACTIVATION, cause::DEACTIVATION_CONFIRMATION] {
        assert_eq!(
            check(
                &one_object(TypeId::C_CI_NA_1, cause_code, &[ADDRESS_ONE, QCC].concat()),
                Direction::Control
            ),
            Err(ProfileError::CauseNotAllowed {
                type_id: TypeId::C_CI_NA_1,
                cause: cause_code,
            })
        );
    }
}

#[test]
fn d004_spontaneous_clock_synchronization_not_permitted() {
    // C_CS_NA_1 (TI 103): spontaneous (3) is marked not permitted.
    assert_eq!(
        check(
            &one_object(
                TypeId::C_CS_NA_1,
                cause::SPONTANEOUS,
                &[ADDRESS_ONE, CP56].concat()
            ),
            Direction::Control
        ),
        Err(ProfileError::CauseNotPermitted {
            type_id: TypeId::C_CS_NA_1,
            cause: cause::SPONTANEOUS,
        })
    );
    assert_eq!(
        check(
            &one_object(
                TypeId::C_CS_NA_1,
                cause::ACTIVATION,
                &[ADDRESS_ONE, CP56].concat()
            ),
            Direction::Control
        ),
        Ok(())
    );
}

#[test]
fn d005_querylog_is_not_sent_with_request() {
    // F_SC_NB_1 (TI 127) is sent with causes 13 and 44 to 47 only: request (5) is not.
    let body = [ADDRESS_ONE, NOF, ZERO_TIME, CP56].concat();
    assert_eq!(
        check(
            &asdu_octets(TypeId::F_SC_NB_1, 1, false, cause::REQUEST, &body),
            Direction::Control
        ),
        Err(ProfileError::CauseNotAllowed {
            type_id: TypeId::F_SC_NB_1,
            cause: cause::REQUEST,
        })
    );
}

#[test]
fn type_outside_the_profile_is_rejected_with_its_octets() {
    // M_SP_TA_1 (type 2) is in the catalogue but not in the profile.
    let bytes = one_object(
        TypeId::M_SP_TA_1,
        cause::SPONTANEOUS,
        &[ADDRESS_ONE, SIQ, CP16].concat(),
    );
    assert_eq!(
        Asdu::decode(&bytes),
        Err(DecodeError::UnsupportedTypeId {
            type_code: 2,
            raw: bytes.clone(),
        })
    );
    assert_eq!(
        validate_type(TypeId::M_SP_TA_1, Direction::Monitor),
        Err(ProfileError::TypeNotInProfile(TypeId::M_SP_TA_1))
    );
}

#[test]
fn wrong_direction_is_reported() {
    let monitor = one_object(
        TypeId::M_SP_NA_1,
        cause::SPONTANEOUS,
        &[ADDRESS_ONE, SIQ].concat(),
    );
    assert_eq!(
        check(&monitor, Direction::Control),
        Err(ProfileError::WrongDirection {
            type_id: TypeId::M_SP_NA_1,
            direction: Direction::Control,
        })
    );
    let command = one_object(
        TypeId::C_SC_NA_1,
        cause::ACTIVATION,
        &[ADDRESS_ONE, SCO].concat(),
    );
    assert_eq!(
        check(&command, Direction::Monitor),
        Err(ProfileError::WrongDirection {
            type_id: TypeId::C_SC_NA_1,
            direction: Direction::Monitor,
        })
    );
}

#[test]
fn sequence_form_is_checked_against_sq_allowed() {
    // M_SP_NA_1 allows SQ = 1, M_SP_TB_1 does not (it has a time tag per object).
    let sequence = asdu_octets(
        TypeId::M_SP_NA_1,
        2,
        true,
        cause::SPONTANEOUS,
        &[ADDRESS_ONE, SIQ, SIQ].concat(),
    );
    assert_eq!(check(&sequence, Direction::Monitor), Ok(()));
    let timed_sequence = asdu_octets(
        TypeId::M_SP_TB_1,
        2,
        true,
        cause::SPONTANEOUS,
        &[ADDRESS_ONE, SIQ, CP56, SIQ, CP56].concat(),
    );
    assert_eq!(
        check(&timed_sequence, Direction::Monitor),
        Err(ProfileError::SequenceNotPermitted(TypeId::M_SP_TB_1))
    );
}

#[test]
fn individual_and_sequence_forms_round_trip() {
    let individual = Asdu {
        cot: CauseOfTransmission::new(cause::SPONTANEOUS).expect("in range"),
        common_address: CommonAddress::new(7),
        body: Body::M_SP_TB_1(Objects::Individual(vec![
            InformationObject {
                address: InformationObjectAddress::new(10).expect("24 bits"),
                value: Timed {
                    value: Siq::decode(SIQ).expect("sample"),
                    time: Cp56Time2a::decode(CP56).expect("sample"),
                },
            },
            InformationObject {
                address: InformationObjectAddress::new(11).expect("24 bits"),
                value: Timed {
                    value: Siq::decode(SIQ).expect("sample"),
                    time: Cp56Time2a::decode(CP56).expect("sample"),
                },
            },
        ])),
    };
    let bytes = individual.to_vec().expect("encodes");
    assert_eq!(Asdu::decode(&bytes), Ok(individual));

    let sequence = Asdu {
        cot: CauseOfTransmission::new(cause::INTERROGATED_STATION).expect("in range"),
        common_address: CommonAddress::new(7),
        body: Body::M_ME_NA_1(Objects::Sequence {
            address: InformationObjectAddress::new(100).expect("24 bits"),
            values: vec![
                (
                    Nva::decode(NVA).expect("sample"),
                    Qds::decode(QDS).expect("sample")
                );
                3
            ],
        }),
    };
    let bytes = sequence.to_vec().expect("encodes");
    assert_eq!(bytes[1], 0x83, "VSQ: SQ = 1, three values");
    assert_eq!(Asdu::decode(&bytes), Ok(sequence));
}

#[test]
fn file_segment_round_trips_with_its_length() {
    let segment = Asdu {
        cot: CauseOfTransmission::new(cause::FILE_TRANSFER).expect("in range"),
        common_address: CommonAddress::new(1),
        body: Body::F_SG_NA_1(Objects::Individual(vec![InformationObject {
            address: InformationObjectAddress::new(0).expect("24 bits"),
            value: FileSegment {
                name_of_file: Nof::decode(NOF).expect("sample"),
                name_of_section: Nos::decode(NOS).expect("sample"),
                segment: crate::elements::Segment::decode(SEGMENT_DATA, 2).expect("two octets"),
            },
        }])),
    };
    let bytes = segment.to_vec().expect("encodes");
    // The segment is the last two octets: the length of segment is 2.
    assert_eq!(&bytes[bytes.len() - 3..], &[0x02, 0xAA, 0x55]);
    assert_eq!(Asdu::decode(&bytes), Ok(segment));
}

#[test]
fn decoding_rejects_a_truncated_object_list() {
    // Two objects of M_SP_NA_1 (4 octets each) but the body holds one.
    let bytes = asdu_octets(
        TypeId::M_SP_NA_1,
        2,
        false,
        cause::SPONTANEOUS,
        &[ADDRESS_ONE, SIQ].concat(),
    );
    assert_eq!(
        Asdu::decode(&bytes),
        Err(DecodeError::Truncated {
            needed: 8,
            available: 4,
        })
    );
}

#[test]
fn decoding_rejects_trailing_octets() {
    let bytes = one_object(
        TypeId::M_SP_NA_1,
        cause::SPONTANEOUS,
        &[ADDRESS_ONE, SIQ, &[0x00][..]].concat(),
    );
    assert_eq!(
        Asdu::decode(&bytes),
        Err(DecodeError::TrailingOctets { extra: 1 })
    );
}

#[test]
fn decoding_rejects_an_asdu_over_249_octets() {
    let body = [ADDRESS_ONE.to_vec(), vec![0; 250]].concat();
    let bytes = asdu_octets(TypeId::F_SG_NA_1, 1, false, cause::FILE_TRANSFER, &body);
    assert_eq!(bytes.len(), 6 + 3 + 250);
    assert_eq!(
        Asdu::decode(&bytes),
        Err(DecodeError::TooLong {
            length: 259,
            max: 249
        })
    );
    assert_eq!(MAX_ASDU_LENGTH, 249);
}

#[test]
fn invalid_element_values_are_reported_by_name() {
    // DCS 0 is not permitted in a double command.
    assert_eq!(
        Asdu::decode(&one_object(
            TypeId::C_DC_NA_1,
            cause::ACTIVATION,
            &[ADDRESS_ONE, &[0x00][..]].concat()
        )),
        Err(DecodeError::InvalidElement { element: "DCO" })
    );
    // A CP56Time2a with month 0 is not a time: the range time reports it.
    assert_eq!(
        Asdu::decode(&asdu_octets(
            TypeId::F_SC_NB_1,
            1,
            false,
            cause::FILE_TRANSFER,
            &[ADDRESS_ONE, NOF, BAD_TIME, CP56].concat(),
        )),
        Err(DecodeError::InvalidElement {
            element: "RangeStartTime"
        })
    );
}

#[test]
fn empty_asdu_round_trips() {
    let bytes = asdu_octets(TypeId::C_RD_NA_1, 0, false, cause::REQUEST, &[]);
    let asdu = Asdu::decode(&bytes).expect("no object");
    assert!(asdu.body.is_empty());
    assert_eq!(asdu.to_vec(), Ok(bytes));
}

#[test]
fn encoding_rejects_too_many_objects_and_oversized_asdus() {
    let object = InformationObject {
        address: InformationObjectAddress::new(1).expect("24 bits"),
        value: Nva::decode(NVA).expect("sample"),
    };
    let too_many = Asdu {
        cot: CauseOfTransmission::new(cause::PERIODIC).expect("in range"),
        common_address: CommonAddress::new(1),
        body: Body::M_ME_ND_1(Objects::Individual(vec![object; 128])),
    };
    assert_eq!(
        too_many.to_vec(),
        Err(EncodeError::TooManyObjects { count: 128 })
    );

    let oversized = Asdu {
        cot: CauseOfTransmission::new(cause::PERIODIC).expect("in range"),
        common_address: CommonAddress::new(1),
        body: Body::M_ME_ND_1(Objects::Individual(vec![
            InformationObject {
                address: InformationObjectAddress::new(1).expect("24 bits"),
                value: Nva::decode(NVA).expect("sample"),
            };
            100
        ])),
    };
    assert_eq!(
        oversized.to_vec(),
        Err(EncodeError::TooLong {
            length: 6 + 100 * 5,
            max: 249
        })
    );
}

#[test]
fn encoding_into_a_short_buffer_fails() {
    let asdu = Asdu::decode(&one_object(
        TypeId::M_SP_NA_1,
        cause::SPONTANEOUS,
        &[ADDRESS_ONE, SIQ].concat(),
    ))
    .expect("decodes");
    assert!(matches!(
        asdu.encode(&mut [0u8; 8]),
        Err(EncodeError::BufferTooSmall { .. })
    ));
}

#[test]
fn profile_rules_are_checked_in_order_and_the_first_failure_is_returned() {
    // C_CS_NA_1 with cause 3 is both not allowed and not permitted: not permitted wins.
    let asdu = Asdu::decode(&one_object(
        TypeId::C_CS_NA_1,
        cause::SPONTANEOUS,
        &[ADDRESS_ONE, CP56].concat(),
    ))
    .expect("decodes");
    assert_eq!(
        validate_profile(&asdu, Direction::Control),
        Err(ProfileError::CauseNotPermitted {
            type_id: TypeId::C_CS_NA_1,
            cause: cause::SPONTANEOUS,
        })
    );
}

#[test]
fn profile_error_messages_name_the_type() {
    let error = ProfileError::CauseNotAllowed {
        type_id: TypeId::M_PS_NA_1,
        cause: 11,
    };
    assert_eq!(error.to_string(), "cause 11 is not allowed for M_PS_NA_1");
}
