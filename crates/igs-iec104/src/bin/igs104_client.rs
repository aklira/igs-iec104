// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! `igs104-client`: a controlling station for IEC 60870-5-104 on the command line
//! (task T3). It connects to a controlled station, runs one procedure, and prints
//! the ASDUs it receives.
//!
//! Each procedure exits with 0 once its confirmations arrive within the timeout,
//! and with 1 otherwise: a refusal by the station, a timeout, or a lost connection.
//! `monitor` starts the data transfer and prints everything until its `--for`
//! duration passes, or until the process is stopped.
//!
//! Times are read from this machine's clock in UTC.

use std::net::SocketAddr;
use std::process::ExitCode;
use std::time::Duration;

use clap::{Parser, Subcommand, ValueEnum};
use igs_iec104::client::{Client, ClientConfig, Event};
use igs_iec104::clock;
use igs_iec104::Delivery;
use igs_iec104_codec::asdu::{Asdu, Body};
use igs_iec104_codec::elements::{
    CounterFreeze, Dco, DoubleCommandState, Nva, Qcc, Qoc, Qoi, Qos, Rco, RegulatingStep, Sco,
    ShortFloat, Sva,
};
use igs_iec104_codec::formats::Cp56Time2a;
use igs_iec104_codec::header::{cause, CommonAddress, InformationObjectAddress};
use igs_iec104_link::TransferState;
use tokio::time::timeout;

/// Connects to a controlled station and runs one procedure.
#[derive(Parser, Debug)]
#[command(
    name = "igs104-client",
    version,
    about = "Controlling station for IEC 60870-5-104: runs one procedure against a controlled station and prints the ASDUs it receives."
)]
struct Cli {
    /// The controlled station, as IP address and port. The standard port is 2404 (§5.4).
    #[arg(short, long, default_value = "127.0.0.1:2404")]
    address: SocketAddr,
    /// The common address of the station's ASDUs.
    #[arg(short = 'c', long, default_value_t = 1)]
    common_address: u16,
    /// Seconds to wait for the connection, and for each confirmation.
    #[arg(short, long, default_value_t = 10)]
    timeout: u64,
    #[command(subcommand)]
    action: Action,
}

/// The procedure to run.
#[derive(Subcommand, Debug)]
enum Action {
    /// Starts the data transfer and prints every ASDU received.
    Monitor {
        /// Stops after this many seconds. Without it, runs until the process is stopped.
        #[arg(long = "for")]
        duration: Option<u64>,
    },
    /// Starts the data transfer, then stops it: STARTDT and STOPDT, each confirmed.
    Stop,
    /// General interrogation (§7.5), of the whole station or of one group (1 to 16).
    Interrogate {
        /// The group, 1 to 16. The whole station when absent.
        #[arg(long)]
        group: Option<u8>,
    },
    /// Counter interrogation (§7.8).
    Counter {
        /// The counter group, 1 to 63.
        #[arg(long, default_value_t = 1)]
        request: u8,
        /// The freeze action.
        #[arg(long, value_enum, default_value_t = FreezeAction::Read)]
        freeze: FreezeAction,
    },
    /// Read of the object at an information object address (§7.2).
    Read {
        /// The information object address.
        address: u32,
    },
    /// Clock synchronization with this machine's time (§7.6).
    ClockSync,
    /// Test command (§7.10): the station answers with the same pattern.
    Test {
        /// The test pattern, any 16-bit value.
        #[arg(long, default_value_t = 0x55AA)]
        pattern: u16,
    },
    /// Single command (§7.7). Run it with `--select`, then without.
    Single {
        /// The information object address.
        address: u32,
        /// Command OFF instead of ON.
        #[arg(long)]
        off: bool,
        /// Select, rather than execute.
        #[arg(long)]
        select: bool,
        /// With a time tag (the time of this machine).
        #[arg(long)]
        time: bool,
    },
    /// Double command (§7.7): OFF or ON.
    Double {
        /// The information object address.
        address: u32,
        /// Double command OFF instead of ON.
        #[arg(long)]
        off: bool,
        /// Select, rather than execute.
        #[arg(long)]
        select: bool,
        /// With a time tag (the time of this machine).
        #[arg(long)]
        time: bool,
    },
    /// Regulating step command (§7.7): one step lower or higher.
    Step {
        /// The information object address.
        address: u32,
        /// One step lower instead of higher.
        #[arg(long)]
        lower: bool,
        /// Select, rather than execute.
        #[arg(long)]
        select: bool,
        /// With a time tag (the time of this machine).
        #[arg(long)]
        time: bool,
    },
    /// Set-point command (§7.7).
    SetPoint {
        /// The information object address.
        address: u32,
        /// The kind of value.
        #[arg(long, value_enum)]
        kind: Kind,
        /// The value: a raw 16-bit number for `normalized` and `scaled`, a number for `float`.
        #[arg(long, allow_hyphen_values = true)]
        value: String,
        /// Select, rather than execute.
        #[arg(long)]
        select: bool,
        /// With a time tag (the time of this machine).
        #[arg(long)]
        time: bool,
    },
}

/// The freeze action of a counter interrogation.
#[derive(ValueEnum, Clone, Copy, Debug)]
enum FreezeAction {
    /// Read the counters, no freeze.
    Read,
    /// Freeze without reset.
    Freeze,
    /// Freeze with reset.
    FreezeReset,
    /// Reset the counters.
    Reset,
}

/// The kind of value of a set-point command.
#[derive(ValueEnum, Clone, Copy, Debug)]
enum Kind {
    /// A normalized value: a raw 16-bit number.
    Normalized,
    /// A scaled value: a raw 16-bit number.
    Scaled,
    /// A short floating point number.
    Float,
}

type Outcome<T = ()> = Result<T, String>;

fn main() -> ExitCode {
    let cli = Cli::parse();
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("igs104-client: no runtime: {error}");
            return ExitCode::FAILURE;
        }
    };
    match runtime.block_on(run(cli)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("igs104-client: {message}");
            ExitCode::FAILURE
        }
    }
}

async fn run(cli: Cli) -> Outcome {
    let confirm = Duration::from_secs(cli.timeout);
    let config = ClientConfig::new(cli.address, CommonAddress::new(cli.common_address));
    let mut client = Client::connect(config);
    match cli.action {
        Action::Monitor { duration } => {
            monitor(&mut client, duration.map(Duration::from_secs)).await
        }
        Action::Stop => {
            start(&mut client, confirm).await?;
            accepted(client.stop_data_transfer().await)?;
            wait_for(&mut client, confirm, "the stop confirmation", judge_stopped).await
        }
        Action::Interrogate { group } => {
            start(&mut client, confirm).await?;
            let group = match group {
                None => Qoi::STATION,
                Some(number) => Qoi::group(number).ok_or("the group is 1 to 16")?,
            };
            accepted(client.interrogate(group).await)?;
            let terminated = judge_answer(cause::ACTIVATION_TERMINATION, |b| {
                matches!(b, Body::C_IC_NA_1(_))
            });
            wait_for(
                &mut client,
                confirm,
                "the end of the interrogation",
                terminated,
            )
            .await
        }
        Action::Counter { request, freeze } => {
            start(&mut client, confirm).await?;
            let freeze = match freeze {
                FreezeAction::Read => CounterFreeze::Read,
                FreezeAction::Freeze => CounterFreeze::FreezeWithoutReset,
                FreezeAction::FreezeReset => CounterFreeze::FreezeWithReset,
                FreezeAction::Reset => CounterFreeze::Reset,
            };
            let qcc = Qcc::new(request, freeze).ok_or("the counter group is 0 to 63")?;
            accepted(client.counter_interrogation(qcc).await)?;
            let terminated = judge_answer(cause::ACTIVATION_TERMINATION, |b| {
                matches!(b, Body::C_CI_NA_1(_))
            });
            wait_for(
                &mut client,
                confirm,
                "the end of the counter interrogation",
                terminated,
            )
            .await
        }
        Action::Read { address } => {
            start(&mut client, confirm).await?;
            accepted(client.read(address_of(address)?).await)?;
            wait_for(
                &mut client,
                confirm,
                "the answer to the read",
                judge_answer(cause::REQUEST, |_| true),
            )
            .await
        }
        Action::ClockSync => {
            start(&mut client, confirm).await?;
            accepted(client.clock_sync(now_utc()?).await)?;
            let confirmed = judge_answer(cause::ACTIVATION_CONFIRMATION, |b| {
                matches!(b, Body::C_CS_NA_1(_))
            });
            wait_for(&mut client, confirm, "the clock confirmation", confirmed).await
        }
        Action::Test { pattern } => {
            start(&mut client, confirm).await?;
            accepted(client.test_command(pattern, now_utc()?).await)?;
            let confirmed = judge_answer(cause::ACTIVATION_CONFIRMATION, |b| {
                matches!(b, Body::C_TS_TA_1(_))
            });
            wait_for(&mut client, confirm, "the test confirmation", confirmed).await
        }
        Action::Single {
            address,
            off,
            select,
            time,
        } => {
            start(&mut client, confirm).await?;
            let command = Sco {
                on: !off,
                qoc: qualifier(select)?,
            };
            accepted(
                client
                    .single_command(address_of(address)?, command, stamp(time)?)
                    .await,
            )?;
            wait_for_execution(&mut client, confirm, select, |b| {
                matches!(b, Body::C_SC_NA_1(_) | Body::C_SC_TA_1(_))
            })
            .await
        }
        Action::Double {
            address,
            off,
            select,
            time,
        } => {
            start(&mut client, confirm).await?;
            let command = Dco {
                state: if off {
                    DoubleCommandState::Off
                } else {
                    DoubleCommandState::On
                },
                qoc: qualifier(select)?,
            };
            accepted(
                client
                    .double_command(address_of(address)?, command, stamp(time)?)
                    .await,
            )?;
            wait_for_execution(&mut client, confirm, select, |b| {
                matches!(b, Body::C_DC_NA_1(_) | Body::C_DC_TA_1(_))
            })
            .await
        }
        Action::Step {
            address,
            lower,
            select,
            time,
        } => {
            start(&mut client, confirm).await?;
            let command = Rco {
                step: if lower {
                    RegulatingStep::Lower
                } else {
                    RegulatingStep::Higher
                },
                qoc: qualifier(select)?,
            };
            accepted(
                client
                    .regulating_step(address_of(address)?, command, stamp(time)?)
                    .await,
            )?;
            wait_for_execution(&mut client, confirm, select, |b| {
                matches!(b, Body::C_RC_NA_1(_) | Body::C_RC_TA_1(_))
            })
            .await
        }
        Action::SetPoint {
            address,
            kind,
            value,
            select,
            time,
        } => {
            start(&mut client, confirm).await?;
            let address = address_of(address)?;
            let qualifier = Qos::new(0, select).ok_or("the qualifier is out of range")?;
            let time = stamp(time)?;
            let sent = match kind {
                Kind::Normalized => {
                    let raw: i16 = value
                        .parse()
                        .map_err(|_| "a normalized value is a 16-bit integer")?;
                    client
                        .set_point_normalized(address, Nva::from_raw(raw), qualifier, time)
                        .await
                }
                Kind::Scaled => {
                    let raw: i16 = value
                        .parse()
                        .map_err(|_| "a scaled value is a 16-bit integer")?;
                    client
                        .set_point_scaled(address, Sva::new(raw), qualifier, time)
                        .await
                }
                Kind::Float => {
                    let number: f32 = value.parse().map_err(|_| "a float value is a number")?;
                    client
                        .set_point_float(address, ShortFloat::from_f32(number), qualifier, time)
                        .await
                }
            };
            accepted(sent)?;
            wait_for_execution(&mut client, confirm, select, |b| {
                matches!(
                    b,
                    Body::C_SE_NA_1(_)
                        | Body::C_SE_TA_1(_)
                        | Body::C_SE_NB_1(_)
                        | Body::C_SE_TB_1(_)
                        | Body::C_SE_NC_1(_)
                        | Body::C_SE_TC_1(_)
                )
            })
            .await
        }
    }
}

/// Starts the data transfer and waits for its confirmation.
async fn start(client: &mut Client, within: Duration) -> Outcome {
    accepted(client.start_data_transfer().await)?;
    wait_for(client, within, "the start confirmation", judge_started).await
}

/// The start is confirmed when the transfer state becomes `Started`.
fn judge_started(event: &Event) -> Option<Outcome> {
    match event {
        Event::Delivery(Delivery::Transfer(TransferState::Started)) => Some(Ok(())),
        _ => None,
    }
}

/// The stop is confirmed when the transfer state becomes `Stopped`.
fn judge_stopped(event: &Event) -> Option<Outcome> {
    match event {
        Event::Delivery(Delivery::Transfer(TransferState::Stopped)) => Some(Ok(())),
        _ => None,
    }
}

/// Starts the data transfer and prints everything received, for `duration` or until the process stops.
async fn monitor(client: &mut Client, duration: Option<Duration>) -> Outcome {
    accepted(client.start_data_transfer().await)?;
    let watch = async {
        while let Some(event) = client.next_event().await {
            print_event(&event);
        }
    };
    match duration {
        Some(duration) => {
            let _ = timeout(duration, watch).await;
        }
        None => watch.await,
    }
    Ok(())
}

/// A command whose select or execution is confirmed: a select by ACTCON, an
/// execution by ACTTERM. A command with a select is answered by the station's
/// ACTCON, then, on execution, by its ACTTERM.
async fn wait_for_execution(
    client: &mut Client,
    within: Duration,
    select: bool,
    kind: fn(&Body) -> bool,
) -> Outcome {
    if select {
        wait_for(
            client,
            within,
            "the select confirmation",
            judge_answer(cause::ACTIVATION_CONFIRMATION, kind),
        )
        .await
    } else {
        wait_for(
            client,
            within,
            "the execution termination",
            judge_answer(cause::ACTIVATION_TERMINATION, kind),
        )
        .await
    }
}

/// Judges an event as the answer to a request. The expected cause confirms it. A
/// negative confirmation (the P/N bit, 101 7.2.3) refuses the request whatever its
/// cause, and so does a negative cause (44 to 47, §8.9). Any other event is not an answer.
fn judge_answer(code: u8, kind: fn(&Body) -> bool) -> impl Fn(&Event) -> Option<Outcome> {
    move |event| match event {
        Event::Delivery(Delivery::Asdu(asdu)) if kind(&asdu.body) => {
            let got = asdu.cot.cause();
            if asdu.cot.negative {
                Some(Err(format!(
                    "the station refused the request: negative {} (cause {got})",
                    cause_name(got)
                )))
            } else if got == code {
                Some(Ok(()))
            } else if (44..=47).contains(&got) {
                Some(Err(format!(
                    "the station refused the request: {} (cause {got})",
                    refusal(got)
                )))
            } else {
                None
            }
        }
        _ => None,
    }
}

/// What a negative cause of transmission says (§8.9).
fn refusal(code: u8) -> &'static str {
    match code {
        44 => "unknown type identification",
        45 => "unknown cause of transmission",
        46 => "unknown common address",
        _ => "unknown information object address",
    }
}

/// Waits, judging each event, until one is an answer, printing every event meanwhile.
async fn wait_for(
    client: &mut Client,
    within: Duration,
    what: &str,
    mut judge: impl FnMut(&Event) -> Option<Outcome>,
) -> Outcome {
    let outcome = timeout(within, async {
        loop {
            let event = client.next_event().await.ok_or("the client stopped")?;
            print_event(&event);
            if let Some(result) = judge(&event) {
                return result;
            }
        }
    })
    .await;
    match outcome {
        Ok(result) => result,
        Err(_) => Err(format!(
            "{what} did not arrive within {} s",
            within.as_secs()
        )),
    }
}

fn print_event(event: &Event) {
    match event {
        Event::Connected => println!("connected"),
        Event::Disconnected(error) => println!("disconnected: {error}"),
        Event::ConnectFailed(error) => println!("connection failed: {error}"),
        Event::Delivery(Delivery::Asdu(asdu)) => println!("{}", describe(asdu)),
        Event::Delivery(Delivery::Rejected(rejection)) => println!("rejected: {rejection:?}"),
        Event::Delivery(Delivery::Transfer(state)) => println!("transfer: {state:?}"),
        Event::Closed => println!("client stopped"),
    }
}

/// One ASDU, decoded: its type, its cause, its common address and its objects.
fn describe(asdu: &Asdu) -> String {
    let code = asdu.cot.cause();
    let polarity = if asdu.cot.negative {
        "negative"
    } else {
        "positive"
    };
    format!(
        "{:?} cause {code} ({}, {polarity}) common address {}: {:?}",
        asdu.type_id(),
        cause_name(code),
        asdu.common_address.value(),
        asdu.body
    )
}

fn cause_name(code: u8) -> &'static str {
    match code {
        cause::SPONTANEOUS => "spontaneous",
        cause::REQUEST => "request",
        cause::ACTIVATION => "activation",
        cause::ACTIVATION_CONFIRMATION => "activation confirmation",
        cause::ACTIVATION_TERMINATION => "activation termination",
        cause::INTERROGATED_STATION => "interrogated by station",
        44..=47 => refusal(code),
        _ => "other",
    }
}

fn accepted<T, E: std::fmt::Display>(result: Result<T, E>) -> Outcome {
    result.map(|_| ()).map_err(|error| error.to_string())
}

fn address_of(value: u32) -> Outcome<InformationObjectAddress> {
    InformationObjectAddress::new(value)
        .ok_or_else(|| "the information object address is out of range".to_string())
}

/// The qualifier of a command: select or execute, with the default qualifier 0.
fn qualifier(select: bool) -> Outcome<Qoc> {
    Qoc::new(0, select).ok_or_else(|| "the qualifier is out of range".to_string())
}

/// The time tag of a command: this machine's time when `wanted`.
fn stamp(wanted: bool) -> Outcome<Option<Cp56Time2a>> {
    if wanted {
        now_utc().map(Some)
    } else {
        Ok(None)
    }
}

/// The current time in UTC, as CP56Time2a (§7.6). Years 2000 to 2099 only.
fn now_utc() -> Outcome<Cp56Time2a> {
    clock::now_utc().map_err(|error| error.to_string())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use igs_iec104_codec::asdu::{InformationObject, Objects};
    use igs_iec104_codec::header::CauseOfTransmission;

    /// An answer to a general interrogation, with the given cause and P/N bit.
    fn interrogation_answer(cause: u8, negative: bool) -> Event {
        let mut cot = CauseOfTransmission::new(cause).expect("a cause of transmission");
        cot.negative = negative;
        let address = InformationObjectAddress::new(0).expect("an address");
        Event::Delivery(Delivery::Asdu(Asdu {
            cot,
            common_address: CommonAddress::new(45),
            body: Body::C_IC_NA_1(Objects::Individual(vec![InformationObject {
                address,
                value: Qoi::STATION,
            }])),
        }))
    }

    fn judge_interrogation() -> impl Fn(&Event) -> Option<Outcome> {
        judge_answer(cause::ACTIVATION_CONFIRMATION, |b| {
            matches!(b, Body::C_IC_NA_1(_))
        })
    }

    #[test]
    fn a_positive_confirmation_is_a_success() {
        let judge = judge_interrogation();
        assert!(matches!(
            judge(&interrogation_answer(cause::ACTIVATION_CONFIRMATION, false)),
            Some(Ok(()))
        ));
    }

    #[test]
    fn a_negative_confirmation_is_a_refusal() {
        let judge = judge_interrogation();
        assert!(matches!(
            judge(&interrogation_answer(cause::ACTIVATION_CONFIRMATION, true)),
            Some(Err(_))
        ));
    }

    #[test]
    fn an_answer_of_another_cause_is_not_judged() {
        let judge = judge_interrogation();
        assert!(judge(&interrogation_answer(cause::ACTIVATION, false)).is_none());
    }
}
