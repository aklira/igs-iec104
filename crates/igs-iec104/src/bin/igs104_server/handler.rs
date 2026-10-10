// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! The application of the demo station: it accepts the commands of the points that take them,
//! and prints each one. A command to any other address is refused as unknown (cause 47).

use std::collections::BTreeSet;

use igs_iec104::server::{Handler, Operation, Refusal};

use super::points::PointSpec;

/// Accepts the commands of the points with `command` set.
pub struct Demo {
    commands: BTreeSet<u32>,
}

impl Demo {
    /// The application for `points`.
    pub fn new(points: &[PointSpec]) -> Self {
        Self {
            commands: points
                .iter()
                .filter(|point| point.command)
                .map(|point| point.address)
                .collect(),
        }
    }

    fn judge(&self, step: &str, operation: &Operation) -> Result<(), Refusal> {
        let address = operation.address.value();
        if self.commands.contains(&address) {
            println!("{step} {:?} at {address}", operation.value);
            Ok(())
        } else {
            Err(Refusal::UnknownAddress)
        }
    }
}

impl Handler for Demo {
    fn select(&self, operation: &Operation) -> Result<(), Refusal> {
        self.judge("select", operation)
    }

    fn execute(&self, operation: &Operation) -> Result<(), Refusal> {
        self.judge("execute", operation)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use igs_iec104::server::CommandValue;
    use igs_iec104_codec::elements::{Qoc, Sco};
    use igs_iec104_codec::header::InformationObjectAddress;

    use super::super::points::{Kind, Simulation};
    use super::*;

    fn point(address: u32, command: bool) -> PointSpec {
        PointSpec {
            address,
            kind: Kind::Single,
            stamped: false,
            group: None,
            command,
            simulation: Simulation::Constant,
        }
    }

    fn operation(address: u32) -> Operation {
        Operation {
            address: InformationObjectAddress::new(address).expect("in range"),
            value: CommandValue::Single(Sco {
                on: true,
                qoc: Qoc::new(0, true).expect("in range"),
            }),
            time: None,
        }
    }

    #[test]
    fn a_command_is_accepted_only_at_a_point_that_takes_commands() {
        let demo = Demo::new(&[point(1, true), point(2, false)]);
        assert_eq!(demo.select(&operation(1)), Ok(()));
        assert_eq!(demo.execute(&operation(1)), Ok(()));
        assert_eq!(demo.select(&operation(2)), Err(Refusal::UnknownAddress));
        assert_eq!(demo.select(&operation(3)), Err(Refusal::UnknownAddress));
    }
}
