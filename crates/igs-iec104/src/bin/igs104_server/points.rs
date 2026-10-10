// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! The points of the demo station: their types, their simulated changes, and their checks.
//!
//! A simulation is a function of the step number, so that a run is repeatable: a point toggles
//! on every odd step, a value ramps by its increment, a counter counts. A constant point holds
//! its zero value.

use std::collections::BTreeSet;
use std::fmt;
use std::time::Duration;

use igs_iec104::process_image::PointValue;
use igs_iec104_codec::elements::{
    Bcr, Diq, DoublePoint, Nva, Qds, QualityFlags, ShortFloat, Siq, Sva,
};
use igs_iec104_codec::header::InformationObjectAddress;

/// The type of a point of the demo station (the monitoring types of the process image).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Single-point information (M_SP).
    Single,
    /// Double-point information (M_DP).
    Double,
    /// Normalized value with quality (M_ME_NA).
    Normalized,
    /// Scaled value with quality (M_ME_NB).
    Scaled,
    /// Short floating point number with quality (M_ME_NC).
    Float,
    /// Integrated total (M_IT).
    Counter,
}

/// How the value of a point changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Simulation {
    /// The value does not change.
    Constant,
    /// Single and double points alternate between their states, once every `period`.
    Toggle {
        /// The time between two changes.
        period: Duration,
    },
    /// Normalized, scaled and float points move by `increment` every `period`, and wrap
    /// around the range of a 16-bit value.
    Ramp {
        /// The time between two changes.
        period: Duration,
        /// The change of the raw value at each step.
        increment: i32,
    },
    /// A counter increases by `increment` every `period`.
    Count {
        /// The time between two changes.
        period: Duration,
        /// The increase of the counter at each step.
        increment: u32,
    },
}

impl Simulation {
    /// The time between two changes, `None` for a constant point.
    pub fn period(self) -> Option<Duration> {
        match self {
            Self::Constant => None,
            Self::Toggle { period } | Self::Ramp { period, .. } | Self::Count { period, .. } => {
                Some(period)
            }
        }
    }
}

/// A point of the demo station.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PointSpec {
    /// The information object address (1 to 2^24 - 1).
    pub address: u32,
    /// The type of the point.
    pub kind: Kind,
    /// The point is time-tagged: its changes carry the time of the change.
    pub stamped: bool,
    /// The group of the point, 1 to 16, for the group interrogations.
    pub group: Option<u8>,
    /// The point accepts commands: the station confirms them and reports them.
    pub command: bool,
    /// How the value of the point changes.
    pub simulation: Simulation,
}

/// A point that the demo cannot serve.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PointError {
    /// The address is 0 or outside the 24 bits of an information object address.
    Address(u32),
    /// Two points have the same address.
    Duplicate(u32),
    /// The simulation does not fit the type of the point.
    Simulation(u32),
    /// The period of a simulation is zero.
    Period(u32),
    /// The group is outside 1 to 16.
    Group(u32),
    /// A counter cannot take commands.
    Command(u32),
}

impl fmt::Display for PointError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Address(address) => write!(f, "point {address}: the address is 1 to 16777215"),
            Self::Duplicate(address) => write!(f, "point {address}: the address is used twice"),
            Self::Simulation(address) => {
                write!(f, "point {address}: the simulation does not fit its type")
            }
            Self::Period(address) => write!(f, "point {address}: the period must be above zero"),
            Self::Group(address) => write!(f, "point {address}: the group is 1 to 16"),
            Self::Command(address) => write!(f, "point {address}: a counter takes no command"),
        }
    }
}

impl std::error::Error for PointError {}

/// Checks the points before the station starts: each address once, a simulation that fits the
/// type, positive periods, groups in range, and no command on a counter.
pub fn validate(points: &[PointSpec]) -> Result<(), PointError> {
    let mut seen = BTreeSet::new();
    for point in points {
        let address = point.address;
        let valid = InformationObjectAddress::new(address).is_some_and(|value| value.value() != 0);
        if !valid {
            return Err(PointError::Address(address));
        }
        if !seen.insert(address) {
            return Err(PointError::Duplicate(address));
        }
        let fits = matches!(
            (point.kind, point.simulation),
            (_, Simulation::Constant)
                | (Kind::Single | Kind::Double, Simulation::Toggle { .. })
                | (
                    Kind::Normalized | Kind::Scaled | Kind::Float,
                    Simulation::Ramp { .. }
                )
                | (Kind::Counter, Simulation::Count { .. })
        );
        if !fits {
            return Err(PointError::Simulation(address));
        }
        if point.simulation.period() == Some(Duration::ZERO) {
            return Err(PointError::Period(address));
        }
        if point.group.is_some_and(|group| !(1..=16).contains(&group)) {
            return Err(PointError::Group(address));
        }
        if point.command && point.kind == Kind::Counter {
            return Err(PointError::Command(address));
        }
    }
    Ok(())
}

/// The value of a point after `step` changes. `None` when the simulation does not fit the type
/// or a number does not fit its field.
pub fn value(kind: Kind, simulation: Simulation, step: u64) -> Option<PointValue> {
    let quality = Qds::decode(&[0])?;
    Some(match kind {
        Kind::Single => PointValue::Single(Siq::decode(&[u8::from(switched(simulation, step)?)])?),
        Kind::Double => PointValue::Double(Diq {
            state: if switched(simulation, step)? {
                DoublePoint::On
            } else {
                DoublePoint::Off
            },
            quality: QualityFlags::default(),
        }),
        Kind::Normalized => PointValue::Normalized(Nva::from_raw(raw(simulation, step)?), quality),
        Kind::Scaled => PointValue::Scaled(Sva::new(raw(simulation, step)?), quality),
        Kind::Float => PointValue::Float(
            ShortFloat::from_f32(f32::from(raw(simulation, step)?)),
            quality,
        ),
        Kind::Counter => PointValue::Counter(counter(simulation, step)?),
    })
}

fn switched(simulation: Simulation, step: u64) -> Option<bool> {
    match simulation {
        Simulation::Constant => Some(false),
        Simulation::Toggle { .. } => Some(step % 2 == 1),
        Simulation::Ramp { .. } | Simulation::Count { .. } => None,
    }
}

/// The raw 16-bit value of a ramp after `step` changes, wrapping around the range.
fn raw(simulation: Simulation, step: u64) -> Option<i16> {
    match simulation {
        Simulation::Constant => Some(0),
        Simulation::Ramp { increment, .. } => {
            let moved = i64::try_from(step)
                .ok()?
                .checked_mul(i64::from(increment))?;
            i16::try_from(moved.rem_euclid(65_536).checked_sub(32_768)?).ok()
        }
        Simulation::Toggle { .. } | Simulation::Count { .. } => None,
    }
}

/// The counter after `step` changes: it counts modulo the largest 31-bit value.
fn counter(simulation: Simulation, step: u64) -> Option<Bcr> {
    let total = match simulation {
        Simulation::Constant => 0,
        Simulation::Count { increment, .. } => step.checked_mul(u64::from(increment))?,
        Simulation::Toggle { .. } | Simulation::Ramp { .. } => return None,
    };
    let counter = i32::try_from(total % 2_147_483_647).ok()?;
    let sequence = u8::try_from(step % 32).ok()?;
    Bcr::new(counter, sequence)
}

/// The points of the demo station, used when no points are given: a single point that toggles
/// and accepts commands, a time-tagged one, a double point, three measured values, a counter, and
/// constant points for the groups.
pub fn demo() -> Vec<PointSpec> {
    let second = |seconds: u64| Duration::from_secs(seconds);
    vec![
        PointSpec {
            address: 1,
            kind: Kind::Single,
            stamped: false,
            group: Some(1),
            command: true,
            simulation: Simulation::Toggle { period: second(5) },
        },
        PointSpec {
            address: 2,
            kind: Kind::Single,
            stamped: true,
            group: None,
            command: false,
            simulation: Simulation::Toggle { period: second(2) },
        },
        PointSpec {
            address: 3,
            kind: Kind::Double,
            stamped: false,
            group: Some(1),
            command: true,
            simulation: Simulation::Toggle { period: second(3) },
        },
        PointSpec {
            address: 4,
            kind: Kind::Normalized,
            stamped: false,
            group: Some(1),
            command: false,
            simulation: Simulation::Ramp {
                period: second(1),
                increment: 1000,
            },
        },
        PointSpec {
            address: 5,
            kind: Kind::Scaled,
            stamped: false,
            group: Some(1),
            command: false,
            simulation: Simulation::Ramp {
                period: second(1),
                increment: -700,
            },
        },
        PointSpec {
            address: 6,
            kind: Kind::Float,
            stamped: true,
            group: Some(1),
            command: false,
            simulation: Simulation::Ramp {
                period: second(2),
                increment: 3,
            },
        },
        PointSpec {
            address: 7,
            kind: Kind::Counter,
            stamped: false,
            group: Some(2),
            command: false,
            simulation: Simulation::Count {
                period: second(10),
                increment: 1,
            },
        },
        PointSpec {
            address: 8,
            kind: Kind::Single,
            stamped: false,
            group: Some(2),
            command: false,
            simulation: Simulation::Constant,
        },
    ]
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn point(address: u32, kind: Kind, simulation: Simulation) -> PointSpec {
        PointSpec {
            address,
            kind,
            stamped: false,
            group: None,
            command: false,
            simulation,
        }
    }

    #[test]
    fn the_demo_points_are_valid() {
        assert_eq!(validate(&demo()), Ok(()));
    }

    #[test]
    fn an_address_is_one_to_the_largest_24_bit_value_and_is_used_once() {
        assert_eq!(
            validate(&[point(0, Kind::Single, Simulation::Constant)]),
            Err(PointError::Address(0))
        );
        assert_eq!(
            validate(&[point(0x0100_0000, Kind::Single, Simulation::Constant)]),
            Err(PointError::Address(0x0100_0000))
        );
        assert_eq!(
            validate(&[
                point(5, Kind::Single, Simulation::Constant),
                point(5, Kind::Double, Simulation::Constant),
            ]),
            Err(PointError::Duplicate(5))
        );
    }

    #[test]
    fn a_simulation_must_fit_the_type() {
        let toggle = Simulation::Toggle {
            period: Duration::from_secs(1),
        };
        assert_eq!(
            validate(&[point(1, Kind::Float, toggle)]),
            Err(PointError::Simulation(1))
        );
        assert_eq!(
            validate(&[point(1, Kind::Counter, toggle)]),
            Err(PointError::Simulation(1))
        );
    }

    #[test]
    fn a_period_is_positive_and_a_group_is_one_to_sixteen() {
        let zero = Simulation::Toggle {
            period: Duration::ZERO,
        };
        assert_eq!(
            validate(&[point(1, Kind::Single, zero)]),
            Err(PointError::Period(1))
        );
        let mut grouped = point(1, Kind::Single, Simulation::Constant);
        grouped.group = Some(17);
        assert_eq!(validate(&[grouped.clone()]), Err(PointError::Group(1)));
        grouped.group = Some(16);
        assert_eq!(validate(&[grouped]), Ok(()));
    }

    #[test]
    fn a_counter_takes_no_command() {
        let mut counter = point(
            1,
            Kind::Counter,
            Simulation::Count {
                period: Duration::from_secs(1),
                increment: 1,
            },
        );
        counter.command = true;
        assert_eq!(validate(&[counter]), Err(PointError::Command(1)));
    }

    #[test]
    fn a_single_point_toggles_on_odd_steps() {
        let toggle = Simulation::Toggle {
            period: Duration::from_secs(1),
        };
        let at = |step| value(Kind::Single, toggle, step);
        assert_eq!(at(0), value(Kind::Single, Simulation::Constant, 0));
        assert_ne!(at(1), at(0));
        assert_eq!(at(2), at(0));
    }

    #[test]
    fn a_ramp_wraps_around_the_range_of_a_16_bit_value() {
        let ramp = Simulation::Ramp {
            period: Duration::from_secs(1),
            increment: 20_000,
        };
        assert_eq!(raw(ramp, 0), Some(-32_768));
        assert_eq!(raw(ramp, 1), Some(-12_768));
        assert_eq!(raw(ramp, 2), Some(7_232));
        assert_eq!(raw(ramp, 3), Some(27_232));
        assert_eq!(raw(ramp, 4), Some(-18_304));
    }

    #[test]
    fn a_counter_counts_and_its_sequence_wraps_at_32() {
        let count = Simulation::Count {
            period: Duration::from_secs(1),
            increment: 3,
        };
        let bcr = counter(count, 33).expect("a counter");
        assert_eq!(bcr.counter(), 99);
        assert_eq!(bcr.sequence(), 1);
    }
}
