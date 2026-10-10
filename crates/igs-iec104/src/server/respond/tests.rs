// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! Unit tests of the responder, without a connection (the integration tests cover the network).

use std::num::NonZeroUsize;
use std::time::{Duration, Instant};

use igs_iec104_codec::asdu::{Asdu, Body, InformationObject, Objects, Timed};
use igs_iec104_codec::elements::{
    Bcr, Bsi, Coi, CounterFreeze, Qcc, Qds, Qoc, Qoi, Qos, Sco, ShortFloat, Siq,
};
use igs_iec104_codec::formats::Cp56Time2a;
use igs_iec104_codec::generated::profile::TypeId;
use igs_iec104_codec::header::{
    cause, CauseOfTransmission, CommonAddress, InformationObjectAddress,
};

use super::{end_of_initialization, CommandValue, Handler, Operation, Refusal, Responder};
use crate::process_image::{PointValue, ProcessImage};

const STATION: u16 = 45;
const TIMEOUT: Duration = Duration::from_secs(30);

fn ca(value: u16) -> CommonAddress {
    CommonAddress::new(value)
}

fn ioa(value: u32) -> InformationObjectAddress {
    InformationObjectAddress::new(value).expect("an address")
}

fn time(milliseconds: u16) -> Cp56Time2a {
    Cp56Time2a::new(milliseconds, 34, 12, 10, 6, 10, 26).expect("a time")
}

fn quality() -> Qds {
    Qds::decode(&[0]).expect("a quality octet")
}

/// The station's points: two in group 1, one counter in group 2, one counter in no group.
fn image() -> ProcessImage {
    let mut image = ProcessImage::new(NonZeroUsize::new(8).expect("a capacity")).expect("an image");
    let station = ca(STATION);
    image
        .add_point(
            station,
            ioa(672),
            PointValue::Single(Siq::decode(&[0x01]).expect("on")),
            false,
        )
        .expect("registered");
    image
        .add_point(
            station,
            ioa(984),
            PointValue::Float(ShortFloat::from_f32(1.5), quality()),
            false,
        )
        .expect("registered");
    let counter = PointValue::Counter(Bcr::decode(&[0; 5]).expect("a counter"));
    image
        .add_point(station, ioa(10), counter, false)
        .expect("registered");
    image
        .add_point(station, ioa(11), counter, false)
        .expect("registered");
    image.set_group(station, ioa(672), 1).expect("a group");
    image.set_group(station, ioa(984), 1).expect("a group");
    image.set_group(station, ioa(10), 2).expect("a group");
    image
}

/// The application of the tests: refuses one address, or every operation when `reject`.
struct Judge {
    unknown: Option<u32>,
    reject: bool,
}

impl Judge {
    fn accepting() -> Self {
        Self {
            unknown: None,
            reject: false,
        }
    }

    fn judge(&self, address: InformationObjectAddress) -> Result<(), Refusal> {
        if self.unknown == Some(address.value()) {
            Err(Refusal::UnknownAddress)
        } else if self.reject {
            Err(Refusal::Rejected)
        } else {
            Ok(())
        }
    }
}

impl Handler for Judge {
    fn select(&self, operation: &Operation) -> Result<(), Refusal> {
        self.judge(operation.address)
    }

    fn execute(&self, operation: &Operation) -> Result<(), Refusal> {
        self.judge(operation.address)
    }

    fn clock_sync(&self, _time: Cp56Time2a) -> Result<(), Refusal> {
        if self.reject {
            Err(Refusal::Rejected)
        } else {
            Ok(())
        }
    }
}

fn request(cause_code: u8, body: Body) -> Asdu {
    Asdu {
        cot: CauseOfTransmission::new(cause_code).expect("a cause"),
        common_address: ca(STATION),
        body,
    }
}

fn one<T>(address: u32, value: T) -> Objects<T> {
    Objects::Individual(vec![InformationObject {
        address: ioa(address),
        value,
    }])
}

fn single_command(address: u32, select: bool) -> Body {
    let qoc = Qoc::new(0, select).expect("a qualifier");
    Body::C_SC_NA_1(one(address, Sco { on: true, qoc }))
}

/// The type, cause, P/N bit and common address of each ASDU, in order.
fn summary(replies: &[Asdu]) -> Vec<(TypeId, u8, bool, u16)> {
    replies
        .iter()
        .map(|asdu| {
            (
                asdu.type_id(),
                asdu.cot.cause(),
                asdu.cot.negative,
                asdu.common_address.value(),
            )
        })
        .collect()
}

fn reply(
    responder: &mut Responder,
    judge: &Judge,
    image: &ProcessImage,
    asdu: &Asdu,
    now: Instant,
) -> Vec<Asdu> {
    responder.respond(judge, image, asdu, now, time(0))
}

fn respond(asdu: &Asdu) -> Vec<Asdu> {
    let mut responder = Responder::new(ca(STATION), TIMEOUT);
    reply(
        &mut responder,
        &Judge::accepting(),
        &image(),
        asdu,
        Instant::now(),
    )
}

#[test]
fn a_general_interrogation_is_confirmed_answered_and_terminated() {
    let asdu = request(cause::ACTIVATION, Body::C_IC_NA_1(one(0, Qoi::STATION)));
    let replies = respond(&asdu);
    // The points come in the order of their addresses: 672, then 984. The profile allows
    // the interrogation cause for no counter, so the counters at 10 and 11 are not in it.
    assert_eq!(
        summary(&replies),
        vec![
            (
                TypeId::C_IC_NA_1,
                cause::ACTIVATION_CONFIRMATION,
                false,
                STATION
            ),
            (
                TypeId::M_SP_NA_1,
                cause::INTERROGATED_STATION,
                false,
                STATION
            ),
            (
                TypeId::M_ME_NC_1,
                cause::INTERROGATED_STATION,
                false,
                STATION
            ),
            (
                TypeId::C_IC_NA_1,
                cause::ACTIVATION_TERMINATION,
                false,
                STATION
            ),
        ]
    );
}

#[test]
fn a_group_interrogation_answers_the_points_of_its_group() {
    let group_one = Qoi::group(1).expect("group 1");
    let replies = respond(&request(
        cause::ACTIVATION,
        Body::C_IC_NA_1(one(0, group_one)),
    ));
    assert_eq!(
        summary(&replies),
        vec![
            (
                TypeId::C_IC_NA_1,
                cause::ACTIVATION_CONFIRMATION,
                false,
                STATION
            ),
            (
                TypeId::M_SP_NA_1,
                cause::INTERROGATED_GROUP_1,
                false,
                STATION
            ),
            (
                TypeId::M_ME_NC_1,
                cause::INTERROGATED_GROUP_1,
                false,
                STATION
            ),
            (
                TypeId::C_IC_NA_1,
                cause::ACTIVATION_TERMINATION,
                false,
                STATION
            ),
        ]
    );
    // Group 2 holds only the counter at address 10, which an interrogation does not carry;
    // group 3 holds nothing. Both are confirmed and terminated.
    let group_two = Qoi::group(2).expect("group 2");
    let replies = respond(&request(
        cause::ACTIVATION,
        Body::C_IC_NA_1(one(0, group_two)),
    ));
    assert_eq!(replies.len(), 2);
    let group_three = Qoi::group(3).expect("group 3");
    let replies = respond(&request(
        cause::ACTIVATION,
        Body::C_IC_NA_1(one(0, group_three)),
    ));
    assert_eq!(replies.len(), 2);
}

#[test]
fn an_interrogation_with_an_unknown_qualifier_is_refused() {
    let qoi = Qoi::new(0);
    let replies = respond(&request(cause::ACTIVATION, Body::C_IC_NA_1(one(0, qoi))));
    assert_eq!(
        summary(&replies),
        vec![(
            TypeId::C_IC_NA_1,
            cause::ACTIVATION_CONFIRMATION,
            true,
            STATION
        )]
    );
}

#[test]
fn a_read_answers_the_point_or_mirrors_an_unknown_address() {
    let known = respond(&request(cause::REQUEST, Body::C_RD_NA_1(one(672, ()))));
    assert_eq!(
        summary(&known),
        vec![(TypeId::M_SP_NA_1, cause::REQUEST, false, STATION)]
    );
    let unknown = respond(&request(cause::REQUEST, Body::C_RD_NA_1(one(999, ()))));
    assert_eq!(
        summary(&unknown),
        vec![(
            TypeId::C_RD_NA_1,
            cause::UNKNOWN_INFO_OBJECT_ADDRESS,
            true,
            STATION
        )]
    );
    assert_eq!(unknown[0].body, Body::C_RD_NA_1(one(999, ())));
    // A counter is read by a counter interrogation only: the profile refuses the request.
    let counter = respond(&request(cause::REQUEST, Body::C_RD_NA_1(one(10, ()))));
    assert_eq!(
        summary(&counter),
        vec![(TypeId::C_RD_NA_1, cause::UNKNOWN_CAUSE, true, STATION)]
    );
}

#[test]
fn a_counter_interrogation_answers_the_counters_and_refuses_freezing() {
    let general = Qcc::new(5, CounterFreeze::Read).expect("a request");
    let replies = respond(&request(
        cause::ACTIVATION,
        Body::C_CI_NA_1(one(0, general)),
    ));
    assert_eq!(
        summary(&replies),
        vec![
            (
                TypeId::C_CI_NA_1,
                cause::ACTIVATION_CONFIRMATION,
                false,
                STATION
            ),
            (TypeId::M_IT_NA_1, cause::COUNTER_GENERAL, false, STATION),
            (TypeId::M_IT_NA_1, cause::COUNTER_GENERAL, false, STATION),
            (
                TypeId::C_CI_NA_1,
                cause::ACTIVATION_TERMINATION,
                false,
                STATION
            ),
        ]
    );
    let group_two = Qcc::new(2, CounterFreeze::Read).expect("a request");
    let replies = respond(&request(
        cause::ACTIVATION,
        Body::C_CI_NA_1(one(0, group_two)),
    ));
    assert_eq!(
        summary(&replies),
        vec![
            (
                TypeId::C_CI_NA_1,
                cause::ACTIVATION_CONFIRMATION,
                false,
                STATION
            ),
            (
                TypeId::M_IT_NA_1,
                cause::COUNTER_GROUP_1 + 1,
                false,
                STATION
            ),
            (
                TypeId::C_CI_NA_1,
                cause::ACTIVATION_TERMINATION,
                false,
                STATION
            ),
        ]
    );
    let freeze = Qcc::new(5, CounterFreeze::FreezeWithReset).expect("a request");
    let replies = respond(&request(cause::ACTIVATION, Body::C_CI_NA_1(one(0, freeze))));
    assert_eq!(
        summary(&replies),
        vec![(
            TypeId::C_CI_NA_1,
            cause::ACTIVATION_CONFIRMATION,
            true,
            STATION
        )]
    );
}

#[test]
fn a_clock_synchronization_is_confirmed_with_the_time_from_before_it() {
    let before = time(111);
    let mut responder = Responder::new(ca(STATION), TIMEOUT);
    let asdu = request(cause::ACTIVATION, Body::C_CS_NA_1(one(0, time(999))));
    let replies = responder.respond(&Judge::accepting(), &image(), &asdu, Instant::now(), before);
    assert_eq!(
        summary(&replies),
        vec![(
            TypeId::C_CS_NA_1,
            cause::ACTIVATION_CONFIRMATION,
            false,
            STATION
        )]
    );
    assert_eq!(replies[0].body, Body::C_CS_NA_1(one(0, before)));

    let refused = responder.respond(
        &Judge {
            unknown: None,
            reject: true,
        },
        &image(),
        &asdu,
        Instant::now(),
        before,
    );
    assert_eq!(
        summary(&refused),
        vec![(
            TypeId::C_CS_NA_1,
            cause::ACTIVATION_CONFIRMATION,
            true,
            STATION
        )]
    );
}

#[test]
fn a_test_command_is_confirmed_with_the_request_it_answers() {
    let body = Body::C_TS_TA_1(Objects::Individual(vec![InformationObject {
        address: ioa(0),
        value: Timed {
            value: igs_iec104_codec::elements::Tsc::new(0x55AA),
            time: time(5),
        },
    }]));
    let asdu = request(cause::ACTIVATION, body.clone());
    let replies = respond(&asdu);
    assert_eq!(
        summary(&replies),
        vec![(
            TypeId::C_TS_TA_1,
            cause::ACTIVATION_CONFIRMATION,
            false,
            STATION
        )]
    );
    assert_eq!(replies[0].body, body);
}

#[test]
fn a_selection_is_confirmed_and_its_execution_terminated() {
    let judge = Judge::accepting();
    let image = image();
    let mut responder = Responder::new(ca(STATION), TIMEOUT);
    let now = Instant::now();
    let select = request(cause::ACTIVATION, single_command(672, true));
    assert_eq!(
        summary(&reply(&mut responder, &judge, &image, &select, now)),
        vec![(
            TypeId::C_SC_NA_1,
            cause::ACTIVATION_CONFIRMATION,
            false,
            STATION
        )]
    );
    let execute = request(cause::ACTIVATION, single_command(672, false));
    assert_eq!(
        summary(&reply(&mut responder, &judge, &image, &execute, now)),
        vec![
            (
                TypeId::C_SC_NA_1,
                cause::ACTIVATION_CONFIRMATION,
                false,
                STATION
            ),
            (
                TypeId::C_SC_NA_1,
                cause::ACTIVATION_TERMINATION,
                false,
                STATION
            ),
        ]
    );
}

#[test]
fn an_execution_consumes_its_selection_and_needs_another_one() {
    let judge = Judge::accepting();
    let image = image();
    let mut responder = Responder::new(ca(STATION), TIMEOUT);
    let now = Instant::now();
    let select = request(cause::ACTIVATION, single_command(672, true));
    let execute = request(cause::ACTIVATION, single_command(672, false));
    reply(&mut responder, &judge, &image, &select, now);
    reply(&mut responder, &judge, &image, &execute, now);
    assert_eq!(
        summary(&reply(&mut responder, &judge, &image, &execute, now)),
        vec![(
            TypeId::C_SC_NA_1,
            cause::ACTIVATION_CONFIRMATION,
            true,
            STATION
        )]
    );
}

#[test]
fn a_direct_execution_without_a_selection_is_refused() {
    let execute = request(cause::ACTIVATION, single_command(672, false));
    assert_eq!(
        summary(&respond(&execute)),
        vec![(
            TypeId::C_SC_NA_1,
            cause::ACTIVATION_CONFIRMATION,
            true,
            STATION
        )]
    );
}

#[test]
fn a_selection_expires_after_its_time_out() {
    let judge = Judge::accepting();
    let image = image();
    let start = Instant::now();
    let select = request(cause::ACTIVATION, single_command(672, true));
    let execute = request(cause::ACTIVATION, single_command(672, false));

    let mut late = Responder::new(ca(STATION), TIMEOUT);
    reply(&mut late, &judge, &image, &select, start);
    let after = start
        .checked_add(TIMEOUT)
        .and_then(|at| at.checked_add(Duration::from_secs(1)));
    let after = after.expect("an instant");
    assert_eq!(
        summary(&reply(&mut late, &judge, &image, &execute, after)),
        vec![(
            TypeId::C_SC_NA_1,
            cause::ACTIVATION_CONFIRMATION,
            true,
            STATION
        )]
    );

    let mut in_time = Responder::new(ca(STATION), TIMEOUT);
    reply(&mut in_time, &judge, &image, &select, start);
    let before = start
        .checked_add(Duration::from_secs(29))
        .expect("an instant");
    assert_eq!(
        summary(&reply(&mut in_time, &judge, &image, &execute, before)),
        vec![
            (
                TypeId::C_SC_NA_1,
                cause::ACTIVATION_CONFIRMATION,
                false,
                STATION
            ),
            (
                TypeId::C_SC_NA_1,
                cause::ACTIVATION_TERMINATION,
                false,
                STATION
            ),
        ]
    );
}

#[test]
fn a_command_to_an_unknown_address_is_mirrored_with_cause_47() {
    let judge = Judge {
        unknown: Some(999),
        reject: false,
    };
    let mut responder = Responder::new(ca(STATION), TIMEOUT);
    let select = request(cause::ACTIVATION, single_command(999, true));
    let replies = reply(&mut responder, &judge, &image(), &select, Instant::now());
    assert_eq!(
        summary(&replies),
        vec![(
            TypeId::C_SC_NA_1,
            cause::UNKNOWN_INFO_OBJECT_ADDRESS,
            true,
            STATION
        )]
    );
    assert_eq!(replies[0].body, single_command(999, true));
}

#[test]
fn a_refused_selection_is_a_negative_confirmation() {
    let judge = Judge {
        unknown: None,
        reject: true,
    };
    let mut responder = Responder::new(ca(STATION), TIMEOUT);
    let select = request(cause::ACTIVATION, single_command(672, true));
    assert_eq!(
        summary(&reply(
            &mut responder,
            &judge,
            &image(),
            &select,
            Instant::now()
        )),
        vec![(
            TypeId::C_SC_NA_1,
            cause::ACTIVATION_CONFIRMATION,
            true,
            STATION
        )]
    );
}

#[test]
fn the_selections_of_a_connection_are_bounded() {
    let judge = Judge::accepting();
    let image = image();
    let mut responder = Responder::new(ca(STATION), TIMEOUT);
    let now = Instant::now();
    for address in 1..=16 {
        let select = request(cause::ACTIVATION, single_command(address, true));
        let replies = reply(&mut responder, &judge, &image, &select, now);
        assert!(!replies[0].cot.negative, "selection {address} is accepted");
    }
    let select = request(cause::ACTIVATION, single_command(17, true));
    assert!(
        reply(&mut responder, &judge, &image, &select, now)[0]
            .cot
            .negative
    );
}

#[test]
fn an_unknown_common_address_is_mirrored_with_cause_46() {
    let asdu = Asdu {
        cot: CauseOfTransmission::new(cause::ACTIVATION).expect("a cause"),
        common_address: ca(1),
        body: Body::C_IC_NA_1(one(0, Qoi::STATION)),
    };
    let replies = respond(&asdu);
    assert_eq!(
        summary(&replies),
        vec![(TypeId::C_IC_NA_1, cause::UNKNOWN_COMMON_ADDRESS, true, 1)]
    );
}

#[test]
fn a_request_to_the_global_address_is_answered_by_the_station() {
    let asdu = Asdu {
        cot: CauseOfTransmission::new(cause::ACTIVATION).expect("a cause"),
        common_address: CommonAddress::GLOBAL,
        body: Body::C_IC_NA_1(one(0, Qoi::STATION)),
    };
    let replies = respond(&asdu);
    assert_eq!(replies[0].common_address, ca(STATION));
    // ACTCON, the two points of the station, ACTTERM: the counters are not in a general interrogation.
    assert_eq!(replies.len(), 4);
}

#[test]
fn a_profile_violation_is_mirrored_with_cause_44_or_45() {
    // A monitoring type is not sent by the controlling station: unknown type (44).
    let monitor = Asdu {
        cot: CauseOfTransmission::new(cause::ACTIVATION).expect("a cause"),
        common_address: ca(STATION),
        body: Body::M_SP_NA_1(one(672, Siq::decode(&[0]).expect("a value"))),
    };
    assert_eq!(
        summary(&respond(&monitor)),
        vec![(TypeId::M_SP_NA_1, cause::UNKNOWN_TYPE_ID, true, STATION)]
    );
    // An interrogation is not sent with the spontaneous cause: unknown cause (45).
    let spontaneous = request(cause::SPONTANEOUS, Body::C_IC_NA_1(one(0, Qoi::STATION)));
    assert_eq!(
        summary(&respond(&spontaneous)),
        vec![(TypeId::C_IC_NA_1, cause::UNKNOWN_CAUSE, true, STATION)]
    );
}

#[test]
fn a_deactivation_is_refused_with_a_negative_deactivation_confirmation() {
    let deactivation = request(cause::DEACTIVATION, single_command(672, false));
    assert_eq!(
        summary(&respond(&deactivation)),
        vec![(
            TypeId::C_SC_NA_1,
            cause::DEACTIVATION_CONFIRMATION,
            true,
            STATION
        )]
    );
}

#[test]
fn an_unsupported_command_is_refused_with_a_negative_confirmation() {
    let bitstring = Body::C_BO_NA_1(one(672, Bsi::decode(&[0; 4]).expect("a bitstring")));
    assert_eq!(
        summary(&respond(&request(cause::ACTIVATION, bitstring))),
        vec![(
            TypeId::C_BO_NA_1,
            cause::ACTIVATION_CONFIRMATION,
            true,
            STATION
        )]
    );
}

#[test]
fn set_point_commands_are_selected_and_executed_with_their_qualifier() {
    let judge = Judge::accepting();
    let image = image();
    let mut responder = Responder::new(ca(STATION), TIMEOUT);
    let now = Instant::now();
    let qualifier = Qos::new(0, true).expect("a qualifier");
    let value = igs_iec104_codec::elements::Nva::from_raw(7);
    let select = request(
        cause::ACTIVATION,
        Body::C_SE_NA_1(one(672, (value, qualifier))),
    );
    assert_eq!(
        summary(&reply(&mut responder, &judge, &image, &select, now)),
        vec![(
            TypeId::C_SE_NA_1,
            cause::ACTIVATION_CONFIRMATION,
            false,
            STATION
        )]
    );
    let execute = request(
        cause::ACTIVATION,
        Body::C_SE_NA_1(one(672, (value, Qos::new(0, false).expect("a qualifier")))),
    );
    assert_eq!(
        summary(&reply(&mut responder, &judge, &image, &execute, now)),
        vec![
            (
                TypeId::C_SE_NA_1,
                cause::ACTIVATION_CONFIRMATION,
                false,
                STATION
            ),
            (
                TypeId::C_SE_NA_1,
                cause::ACTIVATION_TERMINATION,
                false,
                STATION
            ),
        ]
    );
    // The command value of the body is read back through the operation.
    let operation = super::operation_of(&select.body).expect("a command");
    assert_eq!(operation.value, CommandValue::Normalized(value, qualifier));
    assert!(operation.value.select());
}

#[test]
fn the_end_of_initialization_reports_its_cause() {
    let coi = Coi::new(1, false).expect("a cause");
    let asdu = end_of_initialization(ca(STATION), coi).expect("an end of initialization");
    assert_eq!(asdu.cot.cause(), cause::INITIALIZED);
    assert!(!asdu.cot.negative);
    assert_eq!(asdu.body, Body::M_EI_NA_1(one(0, coi)));
    assert_eq!(asdu.type_id(), TypeId::M_EI_NA_1);
}

#[test]
fn every_answer_to_an_interrogation_or_a_read_is_a_valid_monitoring_asdu() {
    use igs_iec104_codec::asdu::{validate_profile, Direction};
    let general = Qcc::new(5, CounterFreeze::Read).expect("a request");
    let requests = [
        request(cause::ACTIVATION, Body::C_IC_NA_1(one(0, Qoi::STATION))),
        request(
            cause::ACTIVATION,
            Body::C_IC_NA_1(one(0, Qoi::group(1).expect("group 1"))),
        ),
        request(cause::ACTIVATION, Body::C_CI_NA_1(one(0, general))),
        request(cause::REQUEST, Body::C_RD_NA_1(one(672, ()))),
    ];
    for asdu in requests {
        for answer in respond(&asdu) {
            if !answer.cot.negative
                && answer.type_id() != TypeId::C_IC_NA_1
                && answer.type_id() != TypeId::C_CI_NA_1
            {
                assert!(
                    validate_profile(&answer, Direction::Monitor).is_ok(),
                    "{:?} is not a valid monitoring ASDU",
                    answer.type_id()
                );
            }
        }
    }
}
