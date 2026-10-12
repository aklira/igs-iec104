// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! A client that recovers from a station that is down. It reconnects with a short delay, and it
//! discards a request made while it has no connection, instead of sending it later. It reports the
//! discarded request, then asks for a general interrogation once the connection is up.
//!
//! Start this example while no station listens on the address, so that it reports the failed
//! connection. A system that refuses the connection reports it at once; one that gives no answer
//! reports it when the connection timeout passes. Then start the `server` example on the same
//! address: the client connects, starts the data transfer and asks for the interrogation. Run it with
//! `cargo run --example client_recovery -- 127.0.0.1:2404 1`. On a machine where `127.0.0.1` gives no
//! answer, use an IPv6 address such as `[::1]:2404` for both examples.

use std::error::Error;
use std::net::SocketAddr;
use std::time::Duration;

use igs_iec104::client::{Client, ClientConfig, Event, OfflineRequests, ReconnectPolicy};
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

    // The first retry comes after half a second, and the delay doubles up to five seconds.
    let mut config = ClientConfig::new(address, CommonAddress::new(common))
        .with_offline_requests(OfflineRequests::Discard);
    config.reconnect = ReconnectPolicy {
        initial: Duration::from_millis(500),
        max: Duration::from_secs(5),
    };

    let mut client = Client::connect(config);
    let mut sent_offline = false;
    while let Some(event) = client.next_event().await {
        println!("{event:?}");
        match event {
            Event::ConnectFailed(_) if !sent_offline => {
                // The client has no connection and waits for the next attempt: this request is
                // discarded and reported, not sent later.
                client.interrogate(Qoi::STATION).await?;
                sent_offline = true;
            }
            Event::Discarded(command) => {
                println!("the request made while offline was discarded: {command:?}");
            }
            Event::Connected => client.start_data_transfer().await?,
            Event::Delivery(Delivery::Transfer(TransferState::Started)) => {
                client.interrogate(Qoi::STATION).await?;
            }
            Event::Delivery(Delivery::Asdu(asdu))
                if asdu.cot.cause() == cause::ACTIVATION_TERMINATION =>
            {
                println!("the interrogation of the connected station is over");
                break;
            }
            _ => {}
        }
    }
    Ok(())
}
