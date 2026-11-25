// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! The campaigns of task I3 (issue #3), run on loopback in one process: the controlled station
//! (`igs-iec104` `Server`) and the controlling station (`Client`) talk over TCP on 127.0.0.1.
//!
//! - `load`: a general interrogation of every point of the station, repeated under spontaneous
//!   traffic on the same connection. Reports the percentiles of the time of each interrogation, and
//!   of the time from a change to its reception.
//! - `switchover`: two stations in one redundancy group. The connection that carries the data is
//!   closed during the load, and a second one takes over. Checks that no change is lost, that each
//!   change is delivered at most twice (once, and once resent), and that the changes arrive in order.
//!
//! The reference peers of the interop bench (`bench/README.md`) need Docker and a Java runtime, and
//! are not part of this harness. Each campaign prints a report on standard output; `run.sh` writes
//! the reports under `docs/campaigns/`.

mod load;
mod stats;
mod switchover;

use std::error::Error;
use std::net::SocketAddr;
use std::num::NonZeroUsize;
use std::process::ExitCode;
use std::str::FromStr;

use igs_iec104::process_image::PointValue;
use igs_iec104::server::{Handler, Operation, Refusal, Server, ServerConfig, ServerHandle};
use igs_iec104_codec::elements::{Qds, ShortFloat};
use igs_iec104_codec::formats::Cp56Time2a;
use igs_iec104_codec::header::{CommonAddress, InformationObjectAddress};

/// The common address of the first station of every campaign.
pub const STATION: u16 = 1;
/// The common address of the second station of the switchover campaign.
pub const SECOND: u16 = 2;
/// The number of spontaneous events the station queues: large enough that no change is dropped.
const EVENT_CAPACITY: usize = 1 << 20;

/// Refuses every command: the campaigns send none.
struct Refuse;

impl Handler for Refuse {
    fn select(&self, _operation: &Operation) -> Result<(), Refusal> {
        Err(Refusal::Rejected)
    }

    fn execute(&self, _operation: &Operation) -> Result<(), Refusal> {
        Err(Refusal::Rejected)
    }
}

/// The quality of every point: valid.
pub fn quality() -> Result<Qds, Box<dyn Error>> {
    Ok(Qds::decode(&[0]).ok_or("the quality octet")?)
}

/// The time tag of every change: a fixed time, which the campaigns do not read.
pub fn time() -> Result<Cp56Time2a, Box<dyn Error>> {
    Ok(Cp56Time2a::new(1000, 30, 12, 1, 1, 1, 26).ok_or("the time")?)
}

/// Starts a listener on loopback that serves `common` and `additional`, each with `points`
/// float points at the addresses 1 to `points`, in group 1. Returns the address of the listener
/// and the handle of the first station.
pub async fn start(
    common: u16,
    additional: &[u16],
    points: u32,
) -> Result<(SocketAddr, ServerHandle), Box<dyn Error>> {
    let mut config = ServerConfig::new("127.0.0.1:0".parse()?, CommonAddress::new(common));
    config.event_capacity = NonZeroUsize::new(EVENT_CAPACITY).ok_or("the capacity")?;
    for extra in additional {
        config = config.with_station(CommonAddress::new(*extra));
    }
    let server = Server::bind(config, Refuse).await?;
    let address = server.local_addr()?;
    let handle = server.handle();
    let mut stations = vec![handle.clone()];
    for extra in additional {
        stations.push(
            handle
                .station(CommonAddress::new(*extra))
                .ok_or("a station")?,
        );
    }
    for station in &stations {
        for number in 1..=points {
            let value = PointValue::Float(ShortFloat::from_f32(0.0), quality()?);
            station.add_point(
                InformationObjectAddress::new(number).ok_or("an address")?,
                value,
                false,
            )?;
            station.set_group(
                InformationObjectAddress::new(number).ok_or("an address")?,
                1,
            )?;
        }
    }
    tokio::spawn(server.run());
    Ok((address, handle))
}

/// The value of an option `--name value`, or `default` when it is not given.
pub fn option<T: FromStr>(args: &[String], name: &str, default: T) -> Result<T, Box<dyn Error>>
where
    T::Err: Error + 'static,
{
    match args.iter().position(|arg| arg == name) {
        None => Ok(default),
        Some(index) => {
            let text = args
                .get(index.saturating_add(1))
                .ok_or("a value is missing")?;
            Ok(text.parse::<T>()?)
        }
    }
}

fn usage() -> ExitCode {
    eprintln!(
        "usage: igs-iec104-campaign load [--seconds N] [--points N] [--changing N] [--rate N] [--every-ms N]\n\
         \x20      igs-iec104-campaign switchover [--points N] [--rate N] [--before N] [--after N]"
    );
    ExitCode::from(2)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(campaign) = args.first().cloned() else {
        return usage();
    };
    let rest = args.get(1..).unwrap_or_default().to_vec();
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("no runtime: {error}");
            return ExitCode::FAILURE;
        }
    };
    let result = match campaign.as_str() {
        "load" => runtime.block_on(load::run(&rest)),
        "switchover" => runtime.block_on(switchover::run(&rest)),
        _ => return usage(),
    };
    match result {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(error) => {
            eprintln!("the campaign did not run: {error}");
            ExitCode::FAILURE
        }
    }
}
