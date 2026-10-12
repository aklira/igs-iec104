// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! A controlled station whose application judges every command. The station answers the commands
//! to points 1 to 10 and refuses the others as unknown addresses. It refuses a time-tagged command,
//! a set point out of its limits, and it prints each clock synchronization it accepts.
//!
//! Run it with `cargo run --example server_handler -- 127.0.0.1:2404 1`. The arguments are the
//! address the station listens on and its common address, as for the `server` example. Connect with
//! the `client_commands` example: its points 1 and 2 are among the first ten, so they are accepted.
//! A command to point 11, or a set point of 1000 on point 2, is refused.

use std::error::Error;
use std::net::SocketAddr;

use igs_iec104::server::{CommandValue, Handler, Operation, Refusal, Server, ServerConfig};
use igs_iec104_codec::formats::Cp56Time2a;
use igs_iec104_codec::header::CommonAddress;

/// The first and the last point that accept commands.
const FIRST_POINT: u32 = 1;
const LAST_POINT: u32 = 10;

/// The judgement of the application. Its methods run on the task of a connection, so they do not block.
struct Judge;

impl Judge {
    /// Accepts a command when its point is one of the first ten, it is not time-tagged, and a set
    /// point is within its limits. The same rules apply to a selection and to its execution.
    fn judge(operation: &Operation) -> Result<(), Refusal> {
        let address = operation.address.value();
        if !(FIRST_POINT..=LAST_POINT).contains(&address) {
            return Err(Refusal::UnknownAddress);
        }
        if operation.time.is_some() {
            return Err(Refusal::Rejected);
        }
        let within_limits = match operation.value {
            CommandValue::Single(_) | CommandValue::Double(_) | CommandValue::Step(_) => true,
            CommandValue::Normalized(value, _) => value.as_f32().abs() <= 0.5,
            // unsigned_abs: the absolute value of i16::MIN does not fit in an i16.
            CommandValue::Scaled(value, _) => value.value().unsigned_abs() <= 1000,
            CommandValue::Float(value, _) => value.as_f32().abs() <= 100.0,
        };
        if within_limits {
            Ok(())
        } else {
            Err(Refusal::Rejected)
        }
    }
}

impl Handler for Judge {
    fn select(&self, operation: &Operation) -> Result<(), Refusal> {
        println!(
            "select of {:?} at {}",
            operation.value,
            operation.address.value()
        );
        Self::judge(operation)
    }

    fn execute(&self, operation: &Operation) -> Result<(), Refusal> {
        println!(
            "execute of {:?} at {}",
            operation.value,
            operation.address.value()
        );
        Self::judge(operation)
    }

    fn clock_sync(&self, time: Cp56Time2a) -> Result<(), Refusal> {
        // The default accepts the synchronization too. It is overridden here to show the hook.
        println!("clock synchronization accepted: {time:?}");
        Ok(())
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn Error>> {
    let mut arguments = std::env::args().skip(1);
    let bind: SocketAddr = match arguments.next() {
        Some(text) => text.parse()?,
        None => "127.0.0.1:2404".parse()?,
    };
    let common: u16 = match arguments.next() {
        Some(text) => text.parse()?,
        None => 1,
    };

    let server = Server::bind(ServerConfig::new(bind, CommonAddress::new(common)), Judge).await?;
    println!("listening on {}", server.local_addr()?);
    server.run().await;
    Ok(())
}
