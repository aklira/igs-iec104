// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! The answers of a controlled station to the ASDUs of a controlling station (IEC
//! 60870-5-104 §7.2 and §7.5 to §7.10), without I/O.
//!
//! [`Responder::respond`] takes one ASDU that the link delivered, the process image and
//! the application's [`Handler`], and returns the ASDUs to send in reply. The connection
//! sends them and handles the time-outs. The answers follow the causes of the standard:
//! an activation is confirmed (7), an interrogation answers with its data (20 to 36) and
//! ends with its termination (10), and a refusal is the same confirmation with the P/N bit
//! set. An ASDU the station cannot place is mirrored with cause 44 to 47 (§8.9): unknown
//! type, cause, common address or information object address.
//!
//! Commands use select-before-operate: a select is kept for a time-out, and an execution
//! is accepted only for a selection that is still kept, of the same type identification
//! (so a time-tagged execution needs a time-tagged select) and the same address. The
//! station answers a direct execution without a selection with a negative confirmation.

use std::time::{Duration, Instant};

use igs_iec104_codec::asdu::{
    validate_profile, Asdu, Body, Direction, InformationObject, Objects, ProfileError, Timed,
};
use igs_iec104_codec::elements::{
    Coi, CounterFreeze, Dco, Nva, Qoi, Qos, Rco, Sco, ShortFloat, Sva,
};
use igs_iec104_codec::formats::Cp56Time2a;
use igs_iec104_codec::generated::profile::TypeId;
use igs_iec104_codec::header::{
    cause, CauseOfTransmission, CommonAddress, InformationObjectAddress,
};

use crate::process_image::{PointValue, ProcessImage};

/// The most selections a connection keeps at once. A select beyond it is refused, so that
/// a controlling station cannot fill the memory of the station with selections.
const MAX_SELECTIONS: usize = 16;

/// Why the application refuses an operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// The information object address is not one the station has: cause 47 (§8.9).
    UnknownAddress,
    /// The station refuses the operation: a negative activation confirmation.
    Rejected,
}

/// The application of a controlled station. The station asks it to judge commands and
/// clock synchronizations; the methods run on the connection's task, so they must not block.
pub trait Handler: Send + Sync + 'static {
    /// Judges a select, the first step of select-before-operate. An accepted selection is
    /// kept until its time-out or its execution.
    fn select(&self, operation: &Operation) -> Result<(), Refusal>;

    /// Judges an execution. The station calls it only after an accepted selection of the
    /// same command.
    fn execute(&self, operation: &Operation) -> Result<(), Refusal>;

    /// Judges a clock synchronization (§7.6) with the time of the controlling station. The
    /// default accepts it: the station keeps its own clock.
    fn clock_sync(&self, _time: Cp56Time2a) -> Result<(), Refusal> {
        Ok(())
    }
}

/// The value of a command, with its qualifier (select or execute).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommandValue {
    /// Single command (C_SC).
    Single(Sco),
    /// Double command (C_DC).
    Double(Dco),
    /// Regulating step command (C_RC).
    Step(Rco),
    /// Set-point command, normalized value (C_SE_NA).
    Normalized(Nva, Qos),
    /// Set-point command, scaled value (C_SE_NB).
    Scaled(Sva, Qos),
    /// Set-point command, short floating point number (C_SE_NC).
    Float(ShortFloat, Qos),
}

impl CommandValue {
    /// True for a select, false for an execution.
    pub fn select(self) -> bool {
        match self {
            Self::Single(command) => command.qoc.select(),
            Self::Double(command) => command.qoc.select(),
            Self::Step(command) => command.qoc.select(),
            Self::Normalized(_, qualifier)
            | Self::Scaled(_, qualifier)
            | Self::Float(_, qualifier) => qualifier.select(),
        }
    }
}

/// A command of a controlling station: its information object address, its value, and
/// its time tag when the command is time-tagged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Operation {
    /// The information object address of the command.
    pub address: InformationObjectAddress,
    /// The value and the qualifier of the command.
    pub value: CommandValue,
    /// The time of the command, for the time-tagged types.
    pub time: Option<Cp56Time2a>,
}

/// The answers of one controlled station, for one connection.
#[derive(Debug)]
pub struct Responder {
    station: CommonAddress,
    selection_timeout: Duration,
    selections: Vec<Selection>,
}

#[derive(Debug)]
struct Selection {
    type_id: TypeId,
    address: InformationObjectAddress,
    expires: Instant,
}

impl Responder {
    /// The answers of `station`. A selection expires `selection_timeout` after it is accepted.
    pub fn new(station: CommonAddress, selection_timeout: Duration) -> Self {
        Self {
            station,
            selection_timeout,
            selections: Vec::new(),
        }
    }

    /// The ASDUs to send in reply to `asdu`, which the controlling station sent at `now`.
    /// `clock` is the local time of the station, which a clock synchronization answers
    /// with: the time from before the synchronization (§7.6).
    pub fn respond<H: Handler + ?Sized>(
        &mut self,
        handler: &H,
        image: &ProcessImage,
        asdu: &Asdu,
        now: Instant,
        clock: Cp56Time2a,
    ) -> Vec<Asdu> {
        if let Err(error) = validate_profile(asdu, Direction::Control) {
            return mirror(asdu, profile_cause(error)).into_iter().collect();
        }
        if asdu.common_address != self.station && asdu.common_address != CommonAddress::GLOBAL {
            return mirror(asdu, cause::UNKNOWN_COMMON_ADDRESS)
                .into_iter()
                .collect();
        }
        if asdu.cot.cause() == cause::DEACTIVATION {
            // Deactivation is not supported: the station refuses it (9, negative).
            return answer(self.station, asdu, cause::DEACTIVATION_CONFIRMATION, true)
                .into_iter()
                .collect();
        }
        match &asdu.body {
            Body::C_IC_NA_1(objects) => self.interrogation(asdu, objects, image),
            Body::C_CI_NA_1(objects) => self.counters(asdu, objects, image),
            Body::C_RD_NA_1(objects) => self.read(asdu, objects, image),
            Body::C_CS_NA_1(objects) => self.clock_sync(handler, asdu, objects, clock),
            Body::C_TS_TA_1(_) => answer(self.station, asdu, cause::ACTIVATION_CONFIRMATION, false)
                .into_iter()
                .collect(),
            body => match operation_of(body) {
                Some(operation) => self.operate(handler, asdu, &operation, now),
                None => answer(self.station, asdu, cause::ACTIVATION_CONFIRMATION, true)
                    .into_iter()
                    .collect(),
            },
        }
    }

    /// General and group interrogation (§7.5): the confirmation, the points of the station
    /// (or of the group) with the interrogation's cause, then the termination.
    fn interrogation(
        &self,
        asdu: &Asdu,
        objects: &Objects<Qoi>,
        image: &ProcessImage,
    ) -> Vec<Asdu> {
        let Some((_, qoi)) = single_object(objects) else {
            return refuse_activation(self.station, asdu);
        };
        let (interrogated, group) = if *qoi == Qoi::STATION {
            (cause::INTERROGATED_STATION, None)
        } else {
            match (1..=16u8).find(|number| Qoi::group(*number) == Some(*qoi)) {
                Some(number) => match group_cause(cause::INTERROGATED_GROUP_1, number) {
                    Some(code) => (code, Some(number)),
                    None => return refuse_activation(self.station, asdu),
                },
                None => return refuse_activation(self.station, asdu),
            }
        };
        let points: Vec<(InformationObjectAddress, PointValue)> = match group {
            None => image.points(self.station).collect(),
            Some(number) => image.points_in_group(self.station, number).collect(),
        };
        let mut out: Vec<Asdu> = answer(self.station, asdu, cause::ACTIVATION_CONFIRMATION, false)
            .into_iter()
            .collect();
        for (address, value) in points {
            out.extend(self.point_answer(value, address, interrogated));
        }
        out.extend(answer(
            self.station,
            asdu,
            cause::ACTIVATION_TERMINATION,
            false,
        ));
        out
    }

    /// Counter interrogation (§7.8): the general request (5) or the groups 1 to 4. The
    /// station answers the read only; freezing and resetting are refused.
    fn counters(
        &self,
        asdu: &Asdu,
        objects: &Objects<igs_iec104_codec::elements::Qcc>,
        image: &ProcessImage,
    ) -> Vec<Asdu> {
        let Some((_, qcc)) = single_object(objects) else {
            return refuse_activation(self.station, asdu);
        };
        if qcc.freeze() != CounterFreeze::Read {
            return refuse_activation(self.station, asdu);
        }
        let (interrogated, group) = match qcc.request() {
            5 => (cause::COUNTER_GENERAL, None),
            number @ 1..=4 => match group_cause(cause::COUNTER_GROUP_1, number) {
                Some(code) => (code, Some(number)),
                None => return refuse_activation(self.station, asdu),
            },
            _ => return refuse_activation(self.station, asdu),
        };
        let points: Vec<(InformationObjectAddress, PointValue)> = match group {
            None => image.points(self.station).collect(),
            Some(number) => image.points_in_group(self.station, number).collect(),
        };
        let mut out: Vec<Asdu> = answer(self.station, asdu, cause::ACTIVATION_CONFIRMATION, false)
            .into_iter()
            .collect();
        for (address, value) in points {
            // Only the counters: the profile allows the counter causes for no other type.
            if let PointValue::Counter(_) = value {
                out.extend(self.point_answer(value, address, interrogated));
            }
        }
        out.extend(answer(
            self.station,
            asdu,
            cause::ACTIVATION_TERMINATION,
            false,
        ));
        out
    }

    /// Read (§7.2): the current value of the point, with the cause of a request (5). A mirror
    /// with cause 47 when the station has no point at the address, and with cause 45 when the
    /// point's type does not allow a request.
    fn read(&self, asdu: &Asdu, objects: &Objects<()>, image: &ProcessImage) -> Vec<Asdu> {
        let Some((address, _)) = single_object(objects) else {
            return refuse_activation(self.station, asdu);
        };
        match image.value(self.station, address) {
            Some(value) => match self.point_answer(value, address, cause::REQUEST) {
                Some(answer) => vec![answer],
                None => mirror(asdu, cause::UNKNOWN_CAUSE).into_iter().collect(),
            },
            None => mirror(asdu, cause::UNKNOWN_INFO_OBJECT_ADDRESS)
                .into_iter()
                .collect(),
        }
    }

    /// The ASDU that carries `value` at `address` with `cause_code`, when the 104 profile
    /// allows that cause for the type of the value. `None` otherwise: the point is not part
    /// of that answer. A time-tagged type is never in an interrogation, and a counter is
    /// only in a counter interrogation.
    fn point_answer(
        &self,
        value: PointValue,
        address: InformationObjectAddress,
        cause_code: u8,
    ) -> Option<Asdu> {
        let asdu = asdu_with(self.station, cause_code, false, value.body(address, None)?)?;
        validate_profile(&asdu, Direction::Monitor)
            .is_ok()
            .then_some(asdu)
    }

    /// Clock synchronization (§7.6): the confirmation carries the station's time from
    /// before the synchronization. A refused synchronization is a negative confirmation.
    fn clock_sync<H: Handler + ?Sized>(
        &self,
        handler: &H,
        asdu: &Asdu,
        objects: &Objects<Cp56Time2a>,
        clock: Cp56Time2a,
    ) -> Vec<Asdu> {
        let Some((address, time)) = single_object(objects) else {
            return refuse_activation(self.station, asdu);
        };
        match handler.clock_sync(*time) {
            Ok(()) => {
                let body = Body::C_CS_NA_1(Objects::Individual(vec![InformationObject {
                    address,
                    value: clock,
                }]));
                asdu_with(self.station, cause::ACTIVATION_CONFIRMATION, false, body)
                    .into_iter()
                    .collect()
            }
            Err(_) => refuse_activation(self.station, asdu),
        }
    }

    /// A command (§7.7), with select-before-operate.
    fn operate<H: Handler + ?Sized>(
        &mut self,
        handler: &H,
        asdu: &Asdu,
        operation: &Operation,
        now: Instant,
    ) -> Vec<Asdu> {
        self.selections.retain(|selection| selection.expires > now);
        let type_id = asdu.type_id();
        let refused = || answer(self.station, asdu, cause::ACTIVATION_CONFIRMATION, true);
        if operation.value.select() {
            if self.selections.len() >= MAX_SELECTIONS {
                return refused().into_iter().collect();
            }
            match handler.select(operation) {
                Err(Refusal::UnknownAddress) => mirror(asdu, cause::UNKNOWN_INFO_OBJECT_ADDRESS)
                    .into_iter()
                    .collect(),
                Err(Refusal::Rejected) => refused().into_iter().collect(),
                Ok(()) => {
                    let Some(expires) = now.checked_add(self.selection_timeout) else {
                        return refused().into_iter().collect();
                    };
                    self.selections
                        .retain(|s| !(s.type_id == type_id && s.address == operation.address));
                    self.selections.push(Selection {
                        type_id,
                        address: operation.address,
                        expires,
                    });
                    answer(self.station, asdu, cause::ACTIVATION_CONFIRMATION, false)
                        .into_iter()
                        .collect()
                }
            }
        } else {
            let Some(index) = self
                .selections
                .iter()
                .position(|s| s.type_id == type_id && s.address == operation.address)
            else {
                return refused().into_iter().collect();
            };
            self.selections.remove(index);
            match handler.execute(operation) {
                Err(Refusal::UnknownAddress) => mirror(asdu, cause::UNKNOWN_INFO_OBJECT_ADDRESS)
                    .into_iter()
                    .collect(),
                Err(Refusal::Rejected) => refused().into_iter().collect(),
                Ok(()) => {
                    let mut out: Vec<Asdu> =
                        answer(self.station, asdu, cause::ACTIVATION_CONFIRMATION, false)
                            .into_iter()
                            .collect();
                    out.extend(answer(
                        self.station,
                        asdu,
                        cause::ACTIVATION_TERMINATION,
                        false,
                    ));
                    out
                }
            }
        }
    }
}

/// The single command or single object of an ASDU; `None` for a sequence or for more
/// than one object.
fn single_object<T>(objects: &Objects<T>) -> Option<(InformationObjectAddress, &T)> {
    match objects {
        Objects::Individual(items) => match items.as_slice() {
            [item] => Some((item.address, &item.value)),
            _ => None,
        },
        Objects::Sequence { .. } => None,
    }
}

/// The command of a command ASDU; `None` for any other ASDU.
fn operation_of(body: &Body) -> Option<Operation> {
    match body {
        Body::C_SC_NA_1(objects) => plain(objects, CommandValue::Single),
        Body::C_SC_TA_1(objects) => timed(objects, CommandValue::Single),
        Body::C_DC_NA_1(objects) => plain(objects, CommandValue::Double),
        Body::C_DC_TA_1(objects) => timed(objects, CommandValue::Double),
        Body::C_RC_NA_1(objects) => plain(objects, CommandValue::Step),
        Body::C_RC_TA_1(objects) => timed(objects, CommandValue::Step),
        Body::C_SE_NA_1(objects) => plain(objects, |(v, q)| CommandValue::Normalized(v, q)),
        Body::C_SE_TA_1(objects) => timed(objects, |(v, q)| CommandValue::Normalized(v, q)),
        Body::C_SE_NB_1(objects) => plain(objects, |(v, q)| CommandValue::Scaled(v, q)),
        Body::C_SE_TB_1(objects) => timed(objects, |(v, q)| CommandValue::Scaled(v, q)),
        Body::C_SE_NC_1(objects) => plain(objects, |(v, q)| CommandValue::Float(v, q)),
        Body::C_SE_TC_1(objects) => timed(objects, |(v, q)| CommandValue::Float(v, q)),
        _ => None,
    }
}

fn plain<T: Copy>(objects: &Objects<T>, value: fn(T) -> CommandValue) -> Option<Operation> {
    let (address, item) = single_object(objects)?;
    Some(Operation {
        address,
        value: value(*item),
        time: None,
    })
}

fn timed<T: Copy>(objects: &Objects<Timed<T>>, value: fn(T) -> CommandValue) -> Option<Operation> {
    let (address, item) = single_object(objects)?;
    Some(Operation {
        address,
        value: value(item.value),
        time: Some(item.time),
    })
}

/// The cause of a group: `first` is the cause of group 1, and group `number` is `number - 1`
/// above it.
fn group_cause(first: u8, number: u8) -> Option<u8> {
    first.checked_add(number.checked_sub(1)?)
}

/// The cause of a profile error: a type the 104 profile does not allow in this direction
/// or form is unknown to the station (44); a cause it does not allow for the type is an
/// unknown cause (45). The SQ = 1 form of a type is treated as an unknown type, so the
/// station does not guess the layout of an object it does not expect.
fn profile_cause(error: ProfileError) -> u8 {
    match error {
        ProfileError::TypeNotInProfile(_)
        | ProfileError::WrongDirection { .. }
        | ProfileError::SequenceNotPermitted(_) => cause::UNKNOWN_TYPE_ID,
        ProfileError::CauseNotPermitted { .. } | ProfileError::CauseNotAllowed { .. } => {
            cause::UNKNOWN_CAUSE
        }
    }
}

/// An ASDU of the given body, cause and P/N bit, from `common`. `None` when the cause is
/// out of range, which the causes of the standard never are.
fn asdu_with(common: CommonAddress, cause_code: u8, negative: bool, body: Body) -> Option<Asdu> {
    let mut cot = CauseOfTransmission::new(cause_code)?;
    cot.negative = negative;
    Some(Asdu {
        cot,
        common_address: common,
        body,
    })
}

/// The confirmation of `asdu` by the station: its body, its cause, from the station.
fn answer(station: CommonAddress, asdu: &Asdu, cause_code: u8, negative: bool) -> Option<Asdu> {
    asdu_with(station, cause_code, negative, asdu.body.clone())
}

/// A negative confirmation of an activation, from the station.
fn refuse_activation(station: CommonAddress, asdu: &Asdu) -> Vec<Asdu> {
    answer(station, asdu, cause::ACTIVATION_CONFIRMATION, true)
        .into_iter()
        .collect()
}

/// The end of initialization (M_EI, §7.3.3.1 of 101): the station's report that it is
/// ready, sent once on a connection after its data transfer starts. `None` only when the
/// cause of transmission is out of range, which the standard's value never is.
pub(crate) fn end_of_initialization(station: CommonAddress, initialization: Coi) -> Option<Asdu> {
    let address = InformationObjectAddress::new(0)?;
    asdu_with(
        station,
        cause::INITIALIZED,
        false,
        Body::M_EI_NA_1(Objects::Individual(vec![InformationObject {
            address,
            value: initialization,
        }])),
    )
}

/// The mirror of `asdu` with a negative cause (44 to 47): the same body and the common
/// address the controlling station used, with the P/N bit set.
fn mirror(asdu: &Asdu, cause_code: u8) -> Option<Asdu> {
    asdu_with(asdu.common_address, cause_code, true, asdu.body.clone())
}

#[cfg(test)]
mod tests;
