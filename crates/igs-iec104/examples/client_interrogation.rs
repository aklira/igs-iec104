// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! A controlling station that runs each kind of request of the client once, in turn: a general
//! interrogation, an interrogation of group 1, a counter interrogation, a read, a clock
//! synchronization and a test command. The next request is sent when the station has answered the
//! one in flight. Each kind is answered in its own way: an interrogation ends with a termination, a
//! read with the answer to it, a clock synchronization and a test command with a confirmation.
//!
//! Run the `server` example first, then `cargo run --example client_interrogation -- 127.0.0.1:2404 1`.
//! The arguments are the address of the station and its common address, as for the `client`
//! example. The station may refuse some requests: a negative confirmation ends a request, and the
//! demonstration prints it and goes on.

use std::error::Error;
use std::net::SocketAddr;

use igs_iec104::client::{Client, ClientConfig, Event};
use igs_iec104::{clock, Delivery};
use igs_iec104_codec::asdu::Asdu;
use igs_iec104_codec::elements::{CounterFreeze, Qcc, Qoi};
use igs_iec104_codec::header::{cause, CommonAddress, InformationObjectAddress};
use igs_iec104_link::TransferState;

/// The point that the read asks for.
const READ_POINT: u32 = 1;
/// The request of all counters in a counter interrogation.
const GENERAL_COUNTER_REQUEST: u8 = 5;
/// The fixed pattern of the test command.
const TEST_PATTERN: u16 = 0x55AA;

/// The requests of the demonstration, in the order they are sent.
#[derive(Clone, Copy, Debug)]
enum Step {
    StationInterrogation,
    GroupInterrogation,
    CounterInterrogation,
    Read,
    ClockSynchronization,
    TestCommand,
}

const STEPS: [Step; 6] = [
    Step::StationInterrogation,
    Step::GroupInterrogation,
    Step::CounterInterrogation,
    Step::Read,
    Step::ClockSynchronization,
    Step::TestCommand,
];

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
    let mut steps = STEPS.iter().copied();
    // The request in flight, if any. A start of the data transfer after a reconnection must not
    // send a second request: the one in flight is still answered.
    let mut in_flight: Option<Step> = None;
    let mut begun = false;
    while let Some(event) = client.next_event().await {
        println!("{event:?}");
        match event {
            Event::Connected => client.start_data_transfer().await?,
            Event::Delivery(Delivery::Transfer(TransferState::Started)) if !begun => {
                begun = true;
                if let Some(step) = steps.next() {
                    send(&client, step).await?;
                    in_flight = Some(step);
                }
            }
            Event::Delivery(Delivery::Asdu(asdu))
                if in_flight.is_some_and(|step| ends(step, &asdu)) =>
            {
                match steps.next() {
                    Some(step) => {
                        send(&client, step).await?;
                        in_flight = Some(step);
                    }
                    None => break,
                }
            }
            Event::Delivery(Delivery::Rejected(rejection)) => {
                println!("the request was not sent: {rejection:?}");
                break;
            }
            _ => {}
        }
    }
    println!("the demonstration is over");
    Ok(())
}

/// Makes the request of `step`.
async fn send(client: &Client, step: Step) -> Result<(), Box<dyn Error>> {
    match step {
        Step::StationInterrogation => client.interrogate(Qoi::STATION).await?,
        Step::GroupInterrogation => {
            client
                .interrogate(Qoi::group(1).ok_or("group 1 is out of range")?)
                .await?;
        }
        Step::CounterInterrogation => {
            let request = Qcc::new(GENERAL_COUNTER_REQUEST, CounterFreeze::Read)
                .ok_or("the counter request is out of range")?;
            client.counter_interrogation(request).await?;
        }
        Step::Read => client.read(point(READ_POINT)?).await?,
        Step::ClockSynchronization => client.clock_sync(clock::now_utc()?).await?,
        Step::TestCommand => client.test_command(TEST_PATTERN, clock::now_utc()?).await?,
    }
    Ok(())
}

/// True when `asdu` is the last answer to the request `step`. A negative confirmation ends any
/// request. The other answers depend on the request: an interrogation ends with its termination,
/// a read with the answer to it (cause 5), and a clock synchronization or a test command with
/// its positive confirmation, which the station sends without a termination.
fn ends(step: Step, asdu: &Asdu) -> bool {
    if asdu.cot.cause() == cause::ACTIVATION_CONFIRMATION && asdu.cot.negative {
        return true;
    }
    let code = asdu.cot.cause();
    match step {
        Step::StationInterrogation | Step::GroupInterrogation | Step::CounterInterrogation => {
            code == cause::ACTIVATION_TERMINATION
        }
        Step::Read => code == cause::REQUEST,
        Step::ClockSynchronization | Step::TestCommand => code == cause::ACTIVATION_CONFIRMATION,
    }
}

fn point(address: u32) -> Result<InformationObjectAddress, Box<dyn Error>> {
    InformationObjectAddress::new(address)
        .ok_or_else(|| format!("address {address} is out of range").into())
}
