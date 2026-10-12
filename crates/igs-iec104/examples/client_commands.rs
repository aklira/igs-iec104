// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! A controlling station that sends commands with select-before-operate. It selects a single
//! command to point 1, executes it once the selection is confirmed, then sends a set point to
//! point 2. The `server` example has no point 2, so the station refuses that command.
//!
//! Run the `server` example first, then `cargo run --example client_commands -- 127.0.0.1:2404 1`.
//! The arguments are the address of the station and its common address, as for the `client`
//! example. Each request waits for the confirmation of the one before it: the station keeps an
//! accepted selection until its time-out or its execution, so the execution must follow the
//! confirmation of the selection.

use std::error::Error;
use std::net::SocketAddr;

use igs_iec104::client::{Client, ClientConfig, Event};
use igs_iec104::Delivery;
use igs_iec104_codec::asdu::Asdu;
use igs_iec104_codec::elements::{Qoc, Qos, Sco, ShortFloat};
use igs_iec104_codec::header::{cause, CommonAddress, InformationObjectAddress};
use igs_iec104_link::TransferState;

/// The point of the `server` example that accepts commands.
const COMMAND_POINT: u32 = 1;
/// A point the `server` example does not have: its set point is refused.
const UNKNOWN_POINT: u32 = 2;

/// What the station is expected to answer next.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Waiting {
    /// Nothing was sent yet.
    Nothing,
    /// The selection of the single command.
    Selection,
    /// The execution of the single command.
    Execution,
    /// The set point to the unknown point.
    Refusal,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn Error>> {
    let mut arguments = std::env::args().skip(1);
    let address: SocketAddr = match arguments.next() {
        Some(text) => text.parse()?,
        None => "127.0.0.1:2404".parse()?,
    };
    let common: u16 = match arguments.next() {
        Some(text) => text.parse()?,
        None => 1,
    };

    let mut client = Client::connect(ClientConfig::new(address, CommonAddress::new(common)));
    let mut waiting = Waiting::Nothing;
    while let Some(event) = client.next_event().await {
        println!("{event:?}");
        match event {
            Event::Connected => client.start_data_transfer().await?,
            Event::Delivery(Delivery::Transfer(TransferState::Started)) => {
                // The first step: select the command, switching it on.
                let select = Sco {
                    on: true,
                    qoc: qoc(true)?,
                };
                client
                    .single_command(point(COMMAND_POINT)?, select, None)
                    .await?;
                waiting = Waiting::Selection;
            }
            Event::Delivery(Delivery::Asdu(asdu)) if is_confirmation(&asdu) => {
                match (waiting, asdu.cot.negative) {
                    // The selection is accepted: execute the same command.
                    (Waiting::Selection, false) => {
                        let execute = Sco {
                            on: true,
                            qoc: qoc(false)?,
                        };
                        client
                            .single_command(point(COMMAND_POINT)?, execute, None)
                            .await?;
                        waiting = Waiting::Execution;
                    }
                    // The execution is accepted: try a set point to a point the station lacks.
                    (Waiting::Execution, false) => {
                        let value = ShortFloat::from_f32(1.5);
                        client
                            .set_point_float(point(UNKNOWN_POINT)?, value, qos()?, None)
                            .await?;
                        waiting = Waiting::Refusal;
                    }
                    // A negative confirmation, or the answer to the last request: the demonstration
                    // ends here.
                    _ => break,
                }
            }
            _ => {}
        }
    }
    println!("the demonstration is over");
    Ok(())
}

/// True for an activation confirmation, positive or negative.
fn is_confirmation(asdu: &Asdu) -> bool {
    asdu.cot.cause() == cause::ACTIVATION_CONFIRMATION
}

fn point(address: u32) -> Result<InformationObjectAddress, Box<dyn Error>> {
    InformationObjectAddress::new(address)
        .ok_or_else(|| format!("address {address} is out of range").into())
}

/// The qualifier of the command: a persistent output, selected or executed.
fn qoc(select: bool) -> Result<Qoc, Box<dyn Error>> {
    Qoc::new(Qoc::PERSISTENT_OUTPUT, select).ok_or_else(|| "the output is out of range".into())
}

/// The qualifier of a set point with no additional definition, executed at once.
fn qos() -> Result<Qos, Box<dyn Error>> {
    Qos::new(0, false).ok_or_else(|| "the qualifier is out of range".into())
}
