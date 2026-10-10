// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! `igs104-server`: a demo controlled station for IEC 60870-5-104 (task S4). It serves a set of
//! points, changes their values as a simulation says, and accepts the commands of the points
//! that take them. It is the device under test of the interop bench.
//!
//! The points come from the built-in demo set, or from a TOML file given with `--points` (see
//! `points.rs`). The station listens on the standard port 2404 unless `--bind` says otherwise, and
//! runs until the process is stopped, or for the seconds of `--for`.

#[path = "igs104_server/handler.rs"]
mod handler;
#[path = "igs104_server/points.rs"]
mod points;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use clap::Parser;
use igs_iec104::clock;
use igs_iec104::server::{Server, ServerConfig, ServerHandle};
use igs_iec104_codec::header::CommonAddress;
use tokio::time::{interval_at, sleep, Instant};

use handler::Demo;
use points::PointSpec;

/// Serves a demo set of points as a controlled station, and simulates their changes.
#[derive(Parser, Debug)]
#[command(name = "igs104-server", version, about)]
struct Cli {
    /// The address the station listens on. The standard port is 2404 (§5.4).
    #[arg(short, long, default_value = "0.0.0.0:2404")]
    bind: SocketAddr,
    /// The common address of the station, 1 to 65534. The global address 65535 is refused.
    #[arg(short = 'c', long, default_value_t = 1)]
    common_address: u16,
    /// Stops after this many seconds. Without it, runs until the process is stopped.
    #[arg(long = "for")]
    duration: Option<u64>,
    /// A TOML file with the points of the station (see docs/igs104-server.md). Without it, the
    /// built-in demo points are served.
    #[arg(short, long, value_name = "FILE")]
    points: Option<PathBuf>,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("igs104-server: {message}");
            ExitCode::FAILURE
        }
    }
}

async fn run(cli: Cli) -> Result<(), String> {
    let common = CommonAddress::new(cli.common_address);
    if common == CommonAddress::GLOBAL {
        return Err("the global common address cannot name the station".to_string());
    }
    let points = match &cli.points {
        Some(path) => {
            let text = std::fs::read_to_string(path)
                .map_err(|error| format!("{}: {error}", path.display()))?;
            points::parse(&text)?
        }
        None => points::demo(),
    };
    points::validate(&points).map_err(|error| error.to_string())?;

    let server = Server::bind(ServerConfig::new(cli.bind, common), Demo::new(&points))
        .await
        .map_err(|error| error.to_string())?;
    let address = server.local_addr().map_err(|error| error.to_string())?;
    println!("igs104-server: station {} on {address}", cli.common_address);

    let handle = server.handle();
    publish(&handle, &points)?;
    for point in points {
        if let Some(period) = point.simulation.period() {
            tokio::spawn(simulate(handle.clone(), point, period));
        }
    }

    let served = server.run();
    match cli.duration {
        Some(seconds) => {
            tokio::select! {
                () = served => {}
                () = sleep(Duration::from_secs(seconds)) => {}
            }
        }
        None => served.await,
    }
    Ok(())
}

/// Registers the points with their initial values and their groups.
fn publish(handle: &ServerHandle, points: &[PointSpec]) -> Result<(), String> {
    for point in points {
        let address = igs_iec104_codec::header::InformationObjectAddress::new(point.address)
            .ok_or_else(|| format!("point {}: the address is out of range", point.address))?;
        let value = points::value(point.kind, point.simulation, 0)
            .ok_or_else(|| format!("point {}: no initial value", point.address))?;
        handle
            .add_point(address, value, point.stamped)
            .map_err(|error| format!("point {}: {error}", point.address))?;
        if let Some(group) = point.group {
            handle
                .set_group(address, group)
                .map_err(|error| format!("point {}: {error}", point.address))?;
        }
    }
    Ok(())
}

/// Changes the value of one point at each period of its simulation, forever.
async fn simulate(handle: ServerHandle, point: PointSpec, period: Duration) {
    let Some(address) = igs_iec104_codec::header::InformationObjectAddress::new(point.address)
    else {
        return;
    };
    // A first change after one period, not at once; a period too long for the clock is never due.
    let Some(first) = Instant::now().checked_add(period) else {
        return;
    };
    let mut ticker = interval_at(first, period);
    let mut step: u64 = 0;
    loop {
        ticker.tick().await;
        step = step.saturating_add(1);
        let Some(value) = points::value(point.kind, point.simulation, step) else {
            continue;
        };
        let Ok(time) = clock::now_utc() else {
            continue;
        };
        if let Err(error) = handle.update(address, value, time) {
            eprintln!("igs104-server: point {}: {error}", point.address);
        }
    }
}
