// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! One running connection of a scenario, and the ASDUs the scenarios send.

use std::time::Duration;

use igs_iec104::{run, Command, Delivery, Transport, TransportError};
use igs_iec104_codec::asdu::{Asdu, Body, InformationObject, Objects};
use igs_iec104_codec::elements::{Qoi, QualityFlags, Siq};
use igs_iec104_codec::header::{
    cause, CauseOfTransmission, CommonAddress, InformationObjectAddress,
};
use igs_iec104_link::{LinkConfig, TransferState};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::timeout;

/// A scenario failure: the message says what was expected and what happened.
pub type Failure = Box<dyn std::error::Error + Send + Sync>;

/// The common address of the probe's own station (the reference peers use 45).
const STATION_ADDRESS: u16 = 1;
/// The global common address, used by the controlling station for interrogation.
const GLOBAL_ADDRESS: u16 = 65535;

/// One connection driven by a scenario: its command and delivery channels, and
/// the task that runs the driver.
pub struct Link {
    commands: mpsc::Sender<Command>,
    deliveries: mpsc::Receiver<Delivery>,
    task: JoinHandle<Result<(), TransportError>>,
}

impl Link {
    pub fn spawn<S: Transport + 'static>(stream: S, config: LinkConfig) -> Self {
        let (commands, commands_rx) = mpsc::channel(8);
        let (deliveries_tx, deliveries) = mpsc::channel(64);
        let task = tokio::spawn(run(stream, config, commands_rx, deliveries_tx));
        Self {
            commands,
            deliveries,
            task,
        }
    }

    /// Sends one command to the driver.
    pub async fn command(&self, command: Command) -> Result<(), Failure> {
        self.commands
            .send(command)
            .await
            .map_err(|_| "the connection ended before a command could be sent".into())
    }

    /// The next delivery, within `within`, that `wanted` accepts; other
    /// deliveries are skipped. A rejection or the end of the connection fails.
    pub async fn expect(
        &mut self,
        what: &str,
        within: Duration,
        wanted: impl Fn(&Delivery) -> bool,
    ) -> Result<Delivery, Failure> {
        let wait = async {
            loop {
                match self.deliveries.recv().await {
                    Some(Delivery::Rejected(rejection)) => {
                        return Err(format!("{what}: unexpected rejection {rejection:?}").into())
                    }
                    Some(delivery) if wanted(&delivery) => return Ok(delivery),
                    Some(_) => {}
                    None => return Err(format!("{what}: the connection ended").into()),
                }
            }
        };
        timeout(within, wait)
            .await
            .map_err(|_| format!("{what}: nothing within {} s", within.as_secs()))?
    }

    /// The next delivery of any kind; `None` once the connection has ended.
    pub async fn next(&mut self) -> Option<Delivery> {
        self.deliveries.recv().await
    }

    /// The next delivery that is already waiting, without waiting for one.
    pub fn try_next(&mut self) -> Option<Delivery> {
        self.deliveries.try_recv().ok()
    }

    /// True once the connection has ended, whatever the reason.
    pub fn ended(&self) -> bool {
        self.task.is_finished()
    }

    /// Ends the connection by dropping the command channel. Fails when the
    /// driver ended with an error.
    pub async fn finish(self) -> Result<(), Failure> {
        drop(self.commands);
        match self.task.await {
            Ok(Ok(())) => Ok(()),
            Ok(Err(error)) => Err(format!("the connection ended with an error: {error}").into()),
            Err(error) => Err(format!("the connection task failed: {error}").into()),
        }
    }
}

fn cause_of(code: u8) -> Result<CauseOfTransmission, Failure> {
    CauseOfTransmission::new(code).ok_or_else(|| "cause out of range".into())
}

fn address_of(value: u32) -> Result<InformationObjectAddress, Failure> {
    InformationObjectAddress::new(value)
        .ok_or_else(|| "information object address out of range".into())
}

/// C_IC_NA_1 general interrogation (station, QOI 20) with the given cause, sent
/// to the global address.
pub fn interrogation(cause_code: u8) -> Result<Asdu, Failure> {
    Ok(Asdu {
        cot: cause_of(cause_code)?,
        common_address: CommonAddress::new(GLOBAL_ADDRESS),
        body: Body::C_IC_NA_1(Objects::Individual(vec![InformationObject {
            address: address_of(0)?,
            value: Qoi::STATION,
        }])),
    })
}

/// The confirmation or the termination of an interrogation: the request's
/// body with the given cause, to the request's common address.
pub fn reply_to(request: &Asdu, cause_code: u8) -> Result<Asdu, Failure> {
    Ok(Asdu {
        cot: cause_of(cause_code)?,
        common_address: request.common_address,
        body: request.body.clone(),
    })
}

/// A spontaneous single-point value (M_SP_NA_1) from the probe's station.
pub fn single_point(address: u32, on: bool) -> Result<Asdu, Failure> {
    Ok(Asdu {
        cot: cause_of(cause::SPONTANEOUS)?,
        common_address: CommonAddress::new(STATION_ADDRESS),
        body: Body::M_SP_NA_1(Objects::Individual(vec![InformationObject {
            address: address_of(address)?,
            value: Siq {
                on,
                quality: QualityFlags::default(),
            },
        }])),
    })
}

/// Matches a delivery of the transfer state `state`.
pub fn transfer_is(state: TransferState) -> impl Fn(&Delivery) -> bool {
    move |delivery| *delivery == Delivery::Transfer(state)
}

/// The cause of an interrogation ASDU (C_IC_NA_1) delivered to the application.
pub fn interrogation_cause(delivery: &Delivery) -> Option<u8> {
    match delivery {
        Delivery::Asdu(asdu) if matches!(asdu.body, Body::C_IC_NA_1(_)) => Some(asdu.cot.cause()),
        _ => None,
    }
}

/// Matches an interrogation ASDU with the cause `cause_code`.
pub fn interrogation_with_cause(cause_code: u8) -> impl Fn(&Delivery) -> bool {
    move |delivery| interrogation_cause(delivery) == Some(cause_code)
}
