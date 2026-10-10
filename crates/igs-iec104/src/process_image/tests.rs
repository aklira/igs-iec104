// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

use std::collections::VecDeque;
use std::num::NonZeroUsize;

use igs_iec104_codec::asdu::{
    validate_profile, Asdu, Body, Direction, InformationObject, Objects, Timed,
};
use igs_iec104_codec::elements::{Bcr, Bsi, Diq, Nva, Qds, Scd, ShortFloat, Siq, Sva, Vti};
use igs_iec104_codec::formats::Cp56Time2a;
use igs_iec104_codec::header::{cause, CommonAddress, InformationObjectAddress};
use proptest::prelude::*;

use super::{PointValue, ProcessError, ProcessImage, Update};

fn ca(value: u16) -> CommonAddress {
    CommonAddress::new(value)
}

fn ioa(value: u32) -> InformationObjectAddress {
    InformationObjectAddress::new(value).expect("an address")
}

fn time(milliseconds: u16) -> Cp56Time2a {
    Cp56Time2a::new(milliseconds, 34, 12, 10, 6, 10, 26).expect("a time")
}

fn image(capacity: usize) -> ProcessImage {
    ProcessImage::new(NonZeroUsize::new(capacity).expect("a capacity")).expect("an image")
}

fn quality() -> Qds {
    Qds::decode(&[0]).expect("a quality octet")
}

fn float(value: f32) -> PointValue {
    PointValue::Float(ShortFloat::from_f32(value), quality())
}

fn single(on: bool) -> PointValue {
    let octet = if on { 0x01 } else { 0x00 };
    PointValue::Single(Siq::decode(&[octet]).expect("a single point octet"))
}

#[test]
fn a_global_address_cannot_address_a_point() {
    let mut image = image(4);
    assert_eq!(
        image.add_point(CommonAddress::GLOBAL, ioa(1), float(0.0), false),
        Err(ProcessError::GlobalAddress)
    );
}

#[test]
fn a_point_is_registered_once() {
    let mut image = image(4);
    image
        .add_point(ca(1), ioa(1), float(0.0), false)
        .expect("registered");
    assert_eq!(
        image.add_point(ca(1), ioa(1), float(1.0), false),
        Err(ProcessError::DuplicatePoint)
    );
    // The same address under another common address is another point.
    assert!(image.add_point(ca(2), ioa(1), float(1.0), false).is_ok());
}

#[test]
fn a_packed_point_cannot_be_time_tagged() {
    let mut image = image(4);
    let packed = PointValue::Packed(Scd::decode(&[0; 4]).expect("packed"), quality());
    assert_eq!(
        image.add_point(ca(1), ioa(1), packed, true),
        Err(ProcessError::NoTimeTag)
    );
    assert!(image.add_point(ca(1), ioa(1), packed, false).is_ok());
}

#[test]
fn an_unknown_point_cannot_be_updated() {
    let mut image = image(4);
    assert_eq!(
        image.update(ca(1), ioa(1), float(0.0), time(0)),
        Err(ProcessError::UnknownPoint)
    );
}

#[test]
fn a_value_of_another_kind_is_refused() {
    let mut image = image(4);
    image
        .add_point(ca(1), ioa(1), float(0.0), false)
        .expect("registered");
    assert_eq!(
        image.update(ca(1), ioa(1), single(true), time(0)),
        Err(ProcessError::KindMismatch)
    );
    assert_eq!(image.value(ca(1), ioa(1)), Some(float(0.0)));
}

#[test]
fn the_initial_value_and_an_unchanged_value_queue_nothing() {
    let mut image = image(4);
    image
        .add_point(ca(1), ioa(1), float(2.5), false)
        .expect("registered");
    assert_eq!(image.pending(), 0);
    assert_eq!(
        image.update(ca(1), ioa(1), float(2.5), time(0)),
        Ok(Update::Unchanged)
    );
    assert_eq!(image.pending(), 0);
}

#[test]
fn a_change_queues_one_spontaneous_asdu() {
    let mut image = image(4);
    image
        .add_point(ca(7), ioa(300), float(0.0), false)
        .expect("registered");
    assert_eq!(
        image.update(ca(7), ioa(300), float(1.5), time(0)),
        Ok(Update::Queued)
    );
    assert_eq!(image.pending(), 1);
    let asdu = image.pop_event().expect("one event");
    assert_eq!(asdu.cot.cause(), cause::SPONTANEOUS);
    assert_eq!(asdu.common_address, ca(7));
    assert_eq!(
        asdu.body,
        Body::M_ME_NC_1(Objects::Individual(vec![InformationObject {
            address: ioa(300),
            value: (ShortFloat::from_f32(1.5), quality()),
        }]))
    );
    assert!(validate_profile(&asdu, Direction::Monitor).is_ok());
    assert_eq!(image.pop_event(), None);
}

#[test]
fn a_time_tagged_point_reports_the_time_of_the_change() {
    let mut image = image(4);
    image
        .add_point(ca(1), ioa(5), single(false), true)
        .expect("registered");
    image
        .update(ca(1), ioa(5), single(true), time(4321))
        .expect("changed");
    let asdu = image.pop_event().expect("one event");
    assert_eq!(
        asdu.body,
        Body::M_SP_TB_1(Objects::Individual(vec![InformationObject {
            address: ioa(5),
            value: Timed {
                value: Siq::decode(&[0x01]).expect("on"),
                time: time(4321),
            },
        }]))
    );
    assert!(validate_profile(&asdu, Direction::Monitor).is_ok());
}

#[test]
fn the_time_of_a_repeated_value_is_not_a_change() {
    let mut image = image(4);
    image
        .add_point(ca(1), ioa(5), single(false), true)
        .expect("registered");
    image
        .update(ca(1), ioa(5), single(true), time(1))
        .expect("changed");
    assert_eq!(
        image.update(ca(1), ioa(5), single(true), time(2)),
        Ok(Update::Unchanged)
    );
    assert_eq!(image.pending(), 1);
}

#[test]
fn the_queue_drops_the_oldest_asdu_when_full_and_counts_it() {
    let mut image = image(2);
    image
        .add_point(ca(1), ioa(1), float(0.0), false)
        .expect("registered");
    for value in [1.0, 2.0, 3.0] {
        image
            .update(ca(1), ioa(1), float(value), time(0))
            .expect("changed");
    }
    assert_eq!(image.pending(), 2);
    assert_eq!(image.dropped(), 1);
    let values: Vec<f32> = std::iter::from_fn(|| image.pop_event())
        .filter_map(|asdu| match asdu.body {
            Body::M_ME_NC_1(Objects::Individual(items)) => {
                items.first().map(|item| item.value.0.as_f32())
            }
            _ => None,
        })
        .collect();
    assert_eq!(values, vec![2.0, 3.0]);
}

#[test]
fn the_points_of_one_common_address_come_in_address_order() {
    let mut image = image(4);
    image
        .add_point(ca(1), ioa(30), float(0.0), false)
        .expect("registered");
    image
        .add_point(ca(1), ioa(10), float(0.0), false)
        .expect("registered");
    image
        .add_point(ca(2), ioa(20), float(0.0), false)
        .expect("registered");
    let addresses: Vec<u32> = image
        .points(ca(1))
        .map(|(address, _)| address.value())
        .collect();
    assert_eq!(addresses, vec![10, 30]);
    assert_eq!(image.points(ca(3)).count(), 0);
}

#[test]
fn a_value_reads_back_after_a_change() {
    let mut image = image(4);
    image
        .add_point(ca(1), ioa(1), float(0.0), false)
        .expect("registered");
    image
        .update(ca(1), ioa(1), float(9.0), time(0))
        .expect("changed");
    assert_eq!(image.value(ca(1), ioa(1)), Some(float(9.0)));
    assert_eq!(image.value(ca(1), ioa(2)), None);
}

/// The kinds of the model's points, as the number `of_kind` takes.
const POINTS: [(u16, u32, u8, bool); 4] = [
    (1, 1, 0, false),
    (1, 2, 6, true),
    (2, 1, 7, false),
    (2, 2, 4, true),
];

/// A value of kind `kind` (0 to 9), decoded from random octets. `None` where the
/// octets are not a valid element, which the property test then skips.
fn of_kind(kind: u8, raw: [u8; 8]) -> Option<PointValue> {
    Some(match kind {
        0 => PointValue::Single(Siq::decode(&raw)?),
        1 => PointValue::Double(Diq::decode(&raw)?),
        2 => PointValue::Step(Vti::decode(&raw)?, Qds::decode(&raw)?),
        3 => PointValue::Bitstring(Bsi::decode(&raw)?, Qds::decode(&raw)?),
        4 => PointValue::Normalized(Nva::decode(&raw)?, Qds::decode(&raw)?),
        5 => PointValue::Scaled(Sva::decode(&raw)?, Qds::decode(&raw)?),
        6 => PointValue::Float(ShortFloat::decode(&raw)?, Qds::decode(&raw)?),
        7 => PointValue::Counter(Bcr::decode(&raw)?),
        8 => PointValue::Packed(Scd::decode(&raw)?, Qds::decode(&raw)?),
        _ => PointValue::NormalizedUnqualified(Nva::decode(&raw)?),
    })
}

#[derive(Clone, Debug)]
enum Op {
    Update { point: usize, raw: [u8; 8] },
    Pop,
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        4 => (0..POINTS.len(), any::<[u8; 8]>()).prop_map(|(point, raw)| Op::Update { point, raw }),
        1 => Just(Op::Pop),
    ]
}

/// The model of the image: the same points, the same change rule and the same
/// drop-oldest queue, written out directly.
struct Model {
    values: Vec<PointValue>,
    queue: VecDeque<Asdu>,
    capacity: usize,
    dropped: u64,
}

impl Model {
    fn new(capacity: usize, initial: Vec<PointValue>) -> Self {
        Self {
            values: initial,
            queue: VecDeque::new(),
            capacity,
            dropped: 0,
        }
    }

    fn update(&mut self, point: usize, value: PointValue) -> Update {
        let Some(&(common, address, _, stamped)) = POINTS.get(point) else {
            return Update::Unchanged;
        };
        let Some(current) = self.values.get_mut(point) else {
            return Update::Unchanged;
        };
        if *current == value {
            return Update::Unchanged;
        }
        *current = value;
        let body = value
            .body(ioa(address), stamped.then_some(time(0)))
            .expect("the model's kinds have a form for their time tag");
        if self.queue.len() >= self.capacity {
            self.queue.pop_front();
            self.dropped = self.dropped.saturating_add(1);
        }
        self.queue.push_back(Asdu {
            cot: igs_iec104_codec::header::CauseOfTransmission::new(cause::SPONTANEOUS)
                .expect("a cause"),
            common_address: ca(common),
            body,
        });
        Update::Queued
    }
}

proptest! {
    /// Over any sequence of updates and pops, the image agrees with the model: the
    /// same updates answer alike, the same ASDUs come out in the same order, the queue
    /// never holds more than its capacity, and every value reads back as the last set.
    #[test]
    fn the_image_agrees_with_the_model(
        capacity in 1usize..=6,
        ops in proptest::collection::vec(op(), 0..80),
    ) {
        let mut image = image(capacity);
        let mut initial = Vec::new();
        for (common, address, kind, stamped) in POINTS {
            let value = of_kind(kind, [0; 8]).expect("zero octets are a valid element");
            image.add_point(ca(common), ioa(address), value, stamped).expect("registered");
            initial.push(value);
        }
        let mut model = Model::new(capacity, initial);

        for op in ops {
            match op {
                Op::Update { point, raw } => {
                    let Some((common, address, kind, _)) = POINTS.get(point).copied() else {
                        continue;
                    };
                    let Some(value) = of_kind(kind, raw) else {
                        continue;
                    };
                    let expected = model.update(point, value);
                    let got = image.update(ca(common), ioa(address), value, time(0));
                    prop_assert_eq!(got, Ok(expected));
                }
                Op::Pop => {
                    prop_assert_eq!(image.pop_event(), model.queue.pop_front());
                }
            }
            prop_assert!(image.pending() <= capacity);
            prop_assert_eq!(image.pending(), model.queue.len());
            prop_assert_eq!(image.dropped(), model.dropped);
            for (index, (common, address, _, _)) in POINTS.iter().enumerate() {
                prop_assert_eq!(image.value(ca(*common), ioa(*address)), model.values.get(index).copied());
            }
        }

        while let Some(asdu) = image.pop_event() {
            prop_assert_eq!(asdu.cot.cause(), cause::SPONTANEOUS);
            prop_assert!(validate_profile(&asdu, Direction::Monitor).is_ok());
        }
    }
}
