// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! A controlling station with redundant connections to two or more endpoints of one controlled
//! station. Only one connection carries the data at a time. The group starts the first endpoint
//! that connects and interrogates it. When the started connection is lost, it starts the next
//! connected endpoint, in the order given. It prints every event of the group.
//!
//! Run it with `cargo run --example redundancy -- 127.0.0.1:2404 127.0.0.1:2405`. Each argument
//! is the address of one endpoint. With no argument the example uses the two addresses above. Both
//! endpoints use the common address 1.

use std::error::Error;

use igs_iec104::client::ClientConfig;
use igs_iec104::redundancy::RedundantClient;
use igs_iec104_codec::header::CommonAddress;

/// The common address of the controlled station, the same on every endpoint.
const COMMON: u16 = 1;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn Error>> {
    let mut endpoints = Vec::new();
    for text in std::env::args().skip(1) {
        endpoints.push(ClientConfig::new(text.parse()?, CommonAddress::new(COMMON)));
    }
    if endpoints.is_empty() {
        for text in ["127.0.0.1:2404", "127.0.0.1:2405"] {
            endpoints.push(ClientConfig::new(text.parse()?, CommonAddress::new(COMMON)));
        }
    }

    let mut group = RedundantClient::connect(endpoints)?;
    // The group ends only when it is dropped; it keeps the connections up on its own.
    while let Some(event) = group.next_event().await {
        println!("{event:?}");
    }
    Ok(())
}
