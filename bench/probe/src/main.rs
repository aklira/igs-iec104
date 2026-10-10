// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! Interop probe for the bench (IMPLEMENTATION.md L4). It runs one scenario of
//! igs-iec104 against a reference peer, inside the bench network.
//!
//! Usage: `igs-iec104-probe <scenario> <address>`. A `d1-*` scenario connects to
//! `<address>`; a `d2-*` scenario listens on it. The result is printed as
//! `PASS <scenario>` (exit 0) or `FAIL <scenario>: <reason>` (exit 1).

mod link;
mod scenarios;

use std::net::SocketAddr;
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let (Some(name), Some(text)) = (args.next(), args.next()) else {
        eprintln!("usage: igs-iec104-probe <scenario> <address>");
        return ExitCode::from(2);
    };
    let address: SocketAddr = match text.parse() {
        Ok(address) => address,
        Err(error) => {
            println!("FAIL {name}: the address {text} is not valid: {error}");
            return ExitCode::from(2);
        }
    };
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            println!("FAIL {name}: no runtime: {error}");
            return ExitCode::FAILURE;
        }
    };
    match runtime.block_on(scenarios::run(&name, address)) {
        Ok(()) => {
            println!("PASS {name}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            println!("FAIL {name}: {error}");
            ExitCode::FAILURE
        }
    }
}
