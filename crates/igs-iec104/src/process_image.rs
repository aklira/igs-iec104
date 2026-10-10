// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! The process image of a controlled station (task S1): the points it publishes,
//! their current values, and the queue of spontaneous ASDUs for their changes.
//!
//! A point is addressed by its common address (101 7.2.4) and its information object
//! address (101 7.2.5). An update that changes the value of a point queues one
//! spontaneous ASDU (cause 3) for it, with the time of the change when the point is
//! time-tagged. The queue holds `capacity` ASDUs at most. When it is full, the oldest
//! ASDU is dropped and counted, so the newest change still reaches the controlling
//! station. The image keeps the current value of every point whatever the queue holds,
//! so a general interrogation can recover a state whose event was dropped.

use std::collections::btree_map::Entry;
use std::collections::{BTreeMap, VecDeque};
use std::fmt;
use std::num::NonZeroUsize;

use igs_iec104_codec::asdu::{
    validate_type, Asdu, Body, Direction, InformationObject, Objects, ProfileError, Timed,
};
use igs_iec104_codec::elements::{Bcr, Bsi, Diq, Nva, Qds, Scd, ShortFloat, Siq, Sva, Vti};
use igs_iec104_codec::formats::Cp56Time2a;
use igs_iec104_codec::generated::profile::TypeId;
use igs_iec104_codec::header::{
    cause, CauseOfTransmission, CommonAddress, InformationObjectAddress,
};

/// The value of a point. Each variant is one monitoring type of the 104 profile; the
/// quality is part of the value, so a change of quality is a change of value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointValue {
    /// Single-point information with quality (M_SP).
    Single(Siq),
    /// Double-point information (M_DP).
    Double(Diq),
    /// Step position with transient state and quality (M_ST).
    Step(Vti, Qds),
    /// Bitstring of 32 bits with quality (M_BO).
    Bitstring(Bsi, Qds),
    /// Normalized value with quality (M_ME_NA).
    Normalized(Nva, Qds),
    /// Scaled value with quality (M_ME_NB).
    Scaled(Sva, Qds),
    /// Short floating point number with quality (M_ME_NC).
    Float(ShortFloat, Qds),
    /// Integrated total: a binary counter reading (M_IT).
    Counter(Bcr),
    /// Packed single-point information with status change detection (M_PS). It has no
    /// time-tagged form.
    Packed(Scd, Qds),
    /// Normalized value without quality (M_ME_ND). It has no time-tagged form.
    NormalizedUnqualified(Nva),
}

impl PointValue {
    /// The ASDU type that carries this value: its time-tagged form when `stamped`.
    /// `None` when the type has no such form (M_PS and M_ME_ND have none).
    pub fn type_id(self, stamped: bool) -> Option<TypeId> {
        Some(match (self, stamped) {
            (Self::Single(_), false) => TypeId::M_SP_NA_1,
            (Self::Single(_), true) => TypeId::M_SP_TB_1,
            (Self::Double(_), false) => TypeId::M_DP_NA_1,
            (Self::Double(_), true) => TypeId::M_DP_TB_1,
            (Self::Step(..), false) => TypeId::M_ST_NA_1,
            (Self::Step(..), true) => TypeId::M_ST_TB_1,
            (Self::Bitstring(..), false) => TypeId::M_BO_NA_1,
            (Self::Bitstring(..), true) => TypeId::M_BO_TB_1,
            (Self::Normalized(..), false) => TypeId::M_ME_NA_1,
            (Self::Normalized(..), true) => TypeId::M_ME_TD_1,
            (Self::Scaled(..), false) => TypeId::M_ME_NB_1,
            (Self::Scaled(..), true) => TypeId::M_ME_TE_1,
            (Self::Float(..), false) => TypeId::M_ME_NC_1,
            (Self::Float(..), true) => TypeId::M_ME_TF_1,
            (Self::Counter(_), false) => TypeId::M_IT_NA_1,
            (Self::Counter(_), true) => TypeId::M_IT_TB_1,
            (Self::Packed(..), false) => TypeId::M_PS_NA_1,
            (Self::Packed(..), true) => return None,
            (Self::NormalizedUnqualified(_), false) => TypeId::M_ME_ND_1,
            (Self::NormalizedUnqualified(_), true) => return None,
        })
    }

    /// The ASDU of one object at `address`, time-tagged with `time` when it is `Some`.
    fn body(self, address: InformationObjectAddress, time: Option<Cp56Time2a>) -> Option<Body> {
        Some(match (self, time) {
            (Self::Single(v), None) => Body::M_SP_NA_1(one(address, v)),
            (Self::Single(v), Some(time)) => {
                Body::M_SP_TB_1(one(address, Timed { value: v, time }))
            }
            (Self::Double(v), None) => Body::M_DP_NA_1(one(address, v)),
            (Self::Double(v), Some(time)) => {
                Body::M_DP_TB_1(one(address, Timed { value: v, time }))
            }
            (Self::Step(v, q), None) => Body::M_ST_NA_1(one(address, (v, q))),
            (Self::Step(v, q), Some(time)) => Body::M_ST_TB_1(one(
                address,
                Timed {
                    value: (v, q),
                    time,
                },
            )),
            (Self::Bitstring(v, q), None) => Body::M_BO_NA_1(one(address, (v, q))),
            (Self::Bitstring(v, q), Some(time)) => Body::M_BO_TB_1(one(
                address,
                Timed {
                    value: (v, q),
                    time,
                },
            )),
            (Self::Normalized(v, q), None) => Body::M_ME_NA_1(one(address, (v, q))),
            (Self::Normalized(v, q), Some(time)) => Body::M_ME_TD_1(one(
                address,
                Timed {
                    value: (v, q),
                    time,
                },
            )),
            (Self::Scaled(v, q), None) => Body::M_ME_NB_1(one(address, (v, q))),
            (Self::Scaled(v, q), Some(time)) => Body::M_ME_TE_1(one(
                address,
                Timed {
                    value: (v, q),
                    time,
                },
            )),
            (Self::Float(v, q), None) => Body::M_ME_NC_1(one(address, (v, q))),
            (Self::Float(v, q), Some(time)) => Body::M_ME_TF_1(one(
                address,
                Timed {
                    value: (v, q),
                    time,
                },
            )),
            (Self::Counter(v), None) => Body::M_IT_NA_1(one(address, v)),
            (Self::Counter(v), Some(time)) => {
                Body::M_IT_TB_1(one(address, Timed { value: v, time }))
            }
            (Self::Packed(v, q), None) => Body::M_PS_NA_1(one(address, (v, q))),
            (Self::Packed(..), Some(_)) => return None,
            (Self::NormalizedUnqualified(v), None) => Body::M_ME_ND_1(one(address, v)),
            (Self::NormalizedUnqualified(_), Some(_)) => return None,
        })
    }
}

fn one<T>(address: InformationObjectAddress, value: T) -> Objects<T> {
    Objects::Individual(vec![InformationObject { address, value }])
}

/// A refused point or update. The refusals are data errors of the caller, not
/// protocol errors: nothing here comes from the network.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcessError {
    /// The global common address cannot address a point.
    GlobalAddress,
    /// A point is already registered at this common address and address.
    DuplicatePoint,
    /// No point is registered at this common address and address.
    UnknownPoint,
    /// The value is not of the kind the point was registered with.
    KindMismatch,
    /// The point is time-tagged, and this kind of value has no time-tagged form.
    NoTimeTag,
    /// The type is not sent by a controlled station (104 9.5).
    Profile(ProfileError),
    /// The spontaneous cause of transmission is out of range.
    Cause,
}

impl fmt::Display for ProcessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::GlobalAddress => write!(f, "the global common address cannot address a point"),
            Self::DuplicatePoint => write!(f, "a point is already registered at this address"),
            Self::UnknownPoint => write!(f, "no point is registered at this address"),
            Self::KindMismatch => write!(f, "the value is not of the kind of the point"),
            Self::NoTimeTag => write!(f, "this value has no time-tagged form"),
            Self::Profile(error) => write!(f, "{error}"),
            Self::Cause => write!(f, "the spontaneous cause of transmission is out of range"),
        }
    }
}

impl std::error::Error for ProcessError {}

/// The outcome of an update that was accepted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Update {
    /// The value is the current one: nothing is queued.
    Unchanged,
    /// The value changed: a spontaneous ASDU is queued for it.
    Queued,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Point {
    address: InformationObjectAddress,
    stamped: bool,
    value: PointValue,
}

/// The points of a controlled station and the queue of their spontaneous ASDUs.
#[derive(Debug)]
pub struct ProcessImage {
    /// Keyed by (common address, information object address): the order is the
    /// order of the addresses, which a general interrogation needs.
    points: BTreeMap<(u16, u32), Point>,
    events: VecDeque<Asdu>,
    capacity: usize,
    dropped: u64,
    spontaneous: CauseOfTransmission,
}

fn key(common: CommonAddress, address: InformationObjectAddress) -> (u16, u32) {
    (common.value(), address.value())
}

impl ProcessImage {
    /// An image with no point, whose queue holds `capacity` ASDUs at most.
    pub fn new(capacity: NonZeroUsize) -> Result<Self, ProcessError> {
        Ok(Self {
            points: BTreeMap::new(),
            events: VecDeque::new(),
            capacity: capacity.get(),
            dropped: 0,
            spontaneous: CauseOfTransmission::new(cause::SPONTANEOUS).ok_or(ProcessError::Cause)?,
        })
    }

    /// Registers a point with its initial value. A `stamped` point reports its changes
    /// with the time of the change. The initial value queues nothing.
    pub fn add_point(
        &mut self,
        common: CommonAddress,
        address: InformationObjectAddress,
        value: PointValue,
        stamped: bool,
    ) -> Result<(), ProcessError> {
        if common.is_global() {
            return Err(ProcessError::GlobalAddress);
        }
        let type_id = value.type_id(stamped).ok_or(ProcessError::NoTimeTag)?;
        validate_type(type_id, Direction::Monitor).map_err(ProcessError::Profile)?;
        match self.points.entry(key(common, address)) {
            Entry::Occupied(_) => Err(ProcessError::DuplicatePoint),
            Entry::Vacant(slot) => {
                slot.insert(Point {
                    address,
                    stamped,
                    value,
                });
                Ok(())
            }
        }
    }

    /// Sets the value of a point at `time`. A change queues one spontaneous ASDU; the
    /// same value again queues nothing, whatever the time.
    pub fn update(
        &mut self,
        common: CommonAddress,
        address: InformationObjectAddress,
        value: PointValue,
        time: Cp56Time2a,
    ) -> Result<Update, ProcessError> {
        let point = self
            .points
            .get_mut(&key(common, address))
            .ok_or(ProcessError::UnknownPoint)?;
        if point.value.type_id(false) != value.type_id(false) {
            return Err(ProcessError::KindMismatch);
        }
        if point.value == value {
            return Ok(Update::Unchanged);
        }
        point.value = value;
        let body = value
            .body(address, point.stamped.then_some(time))
            .ok_or(ProcessError::NoTimeTag)?;
        let asdu = Asdu {
            cot: self.spontaneous,
            common_address: common,
            body,
        };
        self.enqueue(asdu);
        Ok(Update::Queued)
    }

    /// The current value of a point.
    pub fn value(
        &self,
        common: CommonAddress,
        address: InformationObjectAddress,
    ) -> Option<PointValue> {
        self.points
            .get(&key(common, address))
            .map(|point| point.value)
    }

    /// The points of one common address, in the order of their addresses.
    pub fn points(
        &self,
        common: CommonAddress,
    ) -> impl Iterator<Item = (InformationObjectAddress, PointValue)> + '_ {
        let common = common.value();
        self.points
            .range((common, u32::MIN)..=(common, u32::MAX))
            .map(|(_, point)| (point.address, point.value))
    }

    /// The oldest queued spontaneous ASDU, which is removed from the queue.
    pub fn pop_event(&mut self) -> Option<Asdu> {
        self.events.pop_front()
    }

    /// The number of queued ASDUs.
    pub fn pending(&self) -> usize {
        self.events.len()
    }

    /// The number of ASDUs dropped because the queue was full.
    pub fn dropped(&self) -> u64 {
        self.dropped
    }

    /// Queues an ASDU. When the queue is full, the oldest ASDU is dropped and counted.
    fn enqueue(&mut self, asdu: Asdu) {
        if self.events.len() >= self.capacity {
            self.events.pop_front();
            self.dropped = self.dropped.saturating_add(1);
        }
        self.events.push_back(asdu);
    }
}

#[cfg(test)]
mod tests;
