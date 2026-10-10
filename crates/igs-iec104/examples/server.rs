// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! A controlled station with two points. Point 1 is a single-point status that changes every
//! second; point 2 is a short floating point measurement that stays at zero. The station accepts
//! the commands to point 1 and refuses the commands to any other address.
//!
//! Run it with `cargo run --example server -- 127.0.0.1:2404 1`. The first argument is the
//! address the station listens on (127.0.0.1:2404 by default), the second its common address
//! (1 by default). Connect to it with the `client` example.

use std::error::Error;
use std::net::SocketAddr;
use std::time::Duration;

use igs_iec104::clock;
use igs_iec104::process_image::PointValue;
use igs_iec104::server::{Handler, Operation, Refusal, Server, ServerConfig, ServerHandle};
use igs_iec104_codec::elements::{Qds, QualityFlags, ShortFloat, Siq};
use igs_iec104_codec::header::{CommonAddress, InformationObjectAddress};

/// The point that changes, and the only one that accepts commands.
const STATUS: u32 = 1;
/// The measurement that stays at zero.
const MEASUREMENT: u32 = 2;

/// Accepts the commands to the status point and refuses the others as unknown addresses.
struct Commands;

impl Commands {
    fn judge(operation: &Operation) -> Result<(), Refusal> {
        if operation.address.value() == STATUS {
            println!("command {:?} at {STATUS}", operation.value);
            Ok(())
        } else {
            Err(Refusal::UnknownAddress)
        }
    }
}

impl Handler for Commands {
    fn select(&self, operation: &Operation) -> Result<(), Refusal> {
        Self::judge(operation)
    }

    fn execute(&self, operation: &Operation) -> Result<(), Refusal> {
        Self::judge(operation)
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

    let server = Server::bind(
        ServerConfig::new(bind, CommonAddress::new(common)),
        Commands,
    )
    .await?;
    println!("listening on {}", server.local_addr()?);

    let handle = server.handle();
    publish(&handle)?;
    tokio::spawn(simulate(handle));

    // Serves connections until the process is stopped.
    server.run().await;
    Ok(())
}

/// Registers the two points with their first values.
fn publish(handle: &ServerHandle) -> Result<(), Box<dyn Error>> {
    let status = point(STATUS)?;
    handle.add_point(status, single(false), false)?;
    handle.add_point(
        point(MEASUREMENT)?,
        PointValue::Float(ShortFloat::from_f32(0.0), Qds::default()),
        false,
    )?;
    Ok(())
}

/// Toggles the status once a second. A failed update is printed and the next one is tried.
async fn simulate(handle: ServerHandle) {
    let Ok(status) = point(STATUS) else {
        return;
    };
    let mut on = false;
    let mut ticker = tokio::time::interval(Duration::from_secs(1));
    loop {
        ticker.tick().await;
        on = !on;
        let Ok(time) = clock::now_utc() else {
            continue;
        };
        if let Err(error) = handle.update(status, single(on), time) {
            eprintln!("update of point {STATUS}: {error}");
        }
    }
}

fn point(address: u32) -> Result<InformationObjectAddress, Box<dyn Error>> {
    InformationObjectAddress::new(address)
        .ok_or_else(|| format!("address {address} is out of range").into())
}

fn single(on: bool) -> PointValue {
    PointValue::Single(Siq {
        on,
        quality: QualityFlags::default(),
    })
}
