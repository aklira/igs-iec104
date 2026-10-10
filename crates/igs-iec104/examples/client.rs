// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! A controlling station that connects to one controlled station, starts the data transfer and
//! asks for a general interrogation. It prints every event until the interrogation is terminated.
//!
//! Run it with `cargo run --example client -- 127.0.0.1:2404 1`. The first argument is the
//! address of the station (127.0.0.1:2404 by default), the second its common address (1 by
//! default).

use std::error::Error;
use std::net::SocketAddr;

use igs_iec104::client::{Client, ClientConfig, Event};
use igs_iec104::Delivery;
use igs_iec104_codec::elements::Qoi;
use igs_iec104_codec::header::{cause, CommonAddress};
use igs_iec104_link::TransferState;

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

    // The client reconnects by itself after a loss; this loop ends only when it is dropped.
    let mut client = Client::connect(ClientConfig::new(address, CommonAddress::new(common)));
    while let Some(event) = client.next_event().await {
        println!("{event:?}");
        match event {
            Event::Connected => client.start_data_transfer().await?,
            Event::Delivery(Delivery::Transfer(TransferState::Started)) => {
                client.interrogate(Qoi::STATION).await?;
            }
            Event::Delivery(Delivery::Asdu(asdu))
                if asdu.cot.cause() == cause::ACTIVATION_TERMINATION =>
            {
                break;
            }
            _ => {}
        }
    }
    Ok(())
}
