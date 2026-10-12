// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! Every event a client reports, with what it means. The client starts the data transfer when the
//! connection is up, asks for a general interrogation each time the transfer starts, and prints
//! each event for one minute. It then stops, which drops the client.
//!
//! Run the `server` example first, then `cargo run --example client_events -- 127.0.0.1:2404 1`.
//! The arguments are the address of the station and its common address, as for the `client`
//! example. Stop the station during the minute to see the connection events; the client reconnects
//! by itself.

use std::error::Error;
use std::net::SocketAddr;
use std::time::Duration;

use igs_iec104::client::{Client, ClientConfig, Event};
use igs_iec104::Delivery;
use igs_iec104_codec::elements::Qoi;
use igs_iec104_codec::header::CommonAddress;
use igs_iec104_link::TransferState;

/// How long the demonstration runs.
const RUN_FOR: Duration = Duration::from_secs(60);

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
    let deadline = tokio::time::sleep(RUN_FOR);
    tokio::pin!(deadline);
    loop {
        let event = tokio::select! {
            event = client.next_event() => match event {
                Some(event) => event,
                None => break,
            },
            () = &mut deadline => break,
        };
        match event {
            Event::Connected => {
                println!("connected: the transfer is stopped until it is started");
                client.start_data_transfer().await?;
            }
            Event::Disconnected(error) => {
                println!("disconnected, the client reconnects after its delay: {error}");
            }
            Event::ConnectFailed(error) => {
                println!("connection failed, the client tries again after its delay: {error}");
            }
            Event::Discarded(command) => {
                println!("a request made while offline was discarded: {command:?}");
            }
            Event::Delivery(Delivery::Transfer(TransferState::Started)) => {
                println!("the data transfer started: asking for a general interrogation");
                client.interrogate(Qoi::STATION).await?;
            }
            Event::Delivery(Delivery::Transfer(TransferState::Stopped)) => {
                println!("the data transfer stopped");
            }
            // The states in between: a start or a stop waits for its confirmation, or for the
            // acknowledgement of the I frames sent before it.
            Event::Delivery(Delivery::Transfer(state)) => {
                println!("the transfer state is {state:?}");
            }
            Event::Delivery(Delivery::Asdu(asdu)) => println!("received: {asdu:?}"),
            Event::Delivery(Delivery::Rejected(rejection)) => {
                println!("a request was not sent: {rejection:?}");
            }
            Event::Delivery(Delivery::Transmitted(asdu)) => {
                println!("sent in an I frame: {asdu:?}");
            }
            Event::Delivery(Delivery::Window) => {
                println!("an acknowledgement opened the window again");
            }
            Event::Delivery(Delivery::Unacknowledged(asdus)) => {
                println!(
                    "{} ASDU(s) were not acknowledged before the connection ended",
                    asdus.len()
                );
            }
            Event::Closed => break,
        }
    }
    println!("the demonstration is over: dropping the client stops it");
    Ok(())
}
