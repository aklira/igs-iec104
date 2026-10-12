// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! A controlled station whose points change while it runs. The application registers four points
//! of four kinds, puts two of them in interrogation groups, and updates them every two seconds.
//! Each update reports whether it queued a spontaneous ASDU: an update with the value the point
//! already holds queues nothing. The station refuses every command.
//!
//! Run it with `cargo run --example server_process_image -- 127.0.0.1:2404 1`. The arguments are the
//! address the station listens on and its common address, as for the `server` example. Connect with
//! the `client_interrogation` example: its general interrogation returns the four points, and its
//! interrogation of group 1 returns the two points of that group.

use std::error::Error;
use std::net::SocketAddr;
use std::time::Duration;

use igs_iec104::clock;
use igs_iec104::process_image::{PointValue, Update};
use igs_iec104::server::{Handler, Operation, Refusal, Server, ServerConfig, ServerHandle};
use igs_iec104_codec::elements::{Diq, DoublePoint, Qds, QualityFlags, ShortFloat, Siq, Sva};
use igs_iec104_codec::header::{CommonAddress, InformationObjectAddress};

const SINGLE: u32 = 1;
const DOUBLE: u32 = 2;
const SCALED: u32 = 3;
const FLOAT: u32 = 4;

/// The interrogation group of the single-point and double-point values.
const GROUP: u8 = 1;

/// The station has no command: every command is refused.
struct NoCommands;

impl Handler for NoCommands {
    fn select(&self, _operation: &Operation) -> Result<(), Refusal> {
        Err(Refusal::Rejected)
    }

    fn execute(&self, _operation: &Operation) -> Result<(), Refusal> {
        Err(Refusal::Rejected)
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
        NoCommands,
    )
    .await?;
    println!("listening on {}", server.local_addr()?);

    let handle = server.handle();
    let points = publish(&handle)?;
    tokio::spawn(change(handle, points));

    server.run().await;
    Ok(())
}

/// Registers the four points with their first values, and returns their addresses in the order of
/// the constants. The float point is time-tagged: its ASDUs carry the time of the update.
fn publish(handle: &ServerHandle) -> Result<[InformationObjectAddress; 4], Box<dyn Error>> {
    let points = [
        point(SINGLE)?,
        point(DOUBLE)?,
        point(SCALED)?,
        point(FLOAT)?,
    ];
    handle.add_point(points[0], single(false), false)?;
    handle.add_point(points[1], double(DoublePoint::Off), false)?;
    handle.add_point(points[2], scaled(0), false)?;
    handle.add_point(points[3], float(0.0), true)?;
    handle.set_group(points[0], GROUP)?;
    handle.set_group(points[1], GROUP)?;
    // The scaled point is in no group: only a general interrogation returns it.
    Ok(points)
}

/// Updates the points every two seconds. The scaled point changes every third step, so two steps
/// out of three report an unchanged value.
async fn change(handle: ServerHandle, points: [InformationObjectAddress; 4]) {
    let [single_point, double_point, scaled_point, float_point] = points;
    let mut ticker = tokio::time::interval(Duration::from_secs(2));
    let mut step: u32 = 0;
    loop {
        ticker.tick().await;
        step = step.wrapping_add(1);
        let Ok(time) = clock::now_utc() else {
            continue;
        };
        let on = step % 2 == 0;
        let double_state = if on {
            DoublePoint::On
        } else {
            DoublePoint::Off
        };
        // A value from 0 to 990 in steps of 10: it fits in an i16.
        let scaled_value = (step.wrapping_div(3) % 100).saturating_mul(10) as i16;
        let updates = [
            handle.update(single_point, single(on), time),
            handle.update(double_point, double(double_state), time),
            handle.update(scaled_point, scaled(scaled_value), time),
            handle.update(float_point, float(step as f32 * 0.5), time),
        ];
        for (address, result) in points.iter().zip(updates) {
            match result {
                Ok(Update::Queued) => {
                    println!(
                        "point {}: changed, sent to the controllers",
                        address.value()
                    );
                }
                Ok(Update::Unchanged) => {
                    println!("point {}: unchanged, nothing sent", address.value());
                }
                Err(error) => eprintln!("point {}: {error}", address.value()),
            }
        }
        if let Some(value) = handle.value(single_point) {
            println!("point {SINGLE} now holds {value:?}");
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

fn double(state: DoublePoint) -> PointValue {
    PointValue::Double(Diq {
        state,
        quality: QualityFlags::default(),
    })
}

fn scaled(value: i16) -> PointValue {
    PointValue::Scaled(Sva::new(value), Qds::default())
}

fn float(value: f32) -> PointValue {
    PointValue::Float(ShortFloat::from_f32(value), Qds::default())
}
