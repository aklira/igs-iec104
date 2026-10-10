// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! The client of a controlled station (T2): a connection kept up by a reconnect
//! policy, the procedures of §7 as methods, and an event stream.
//!
//! A supervisor task connects within t0, runs the session over the connection,
//! and connects again after a drop. The delay between two attempts starts at the
//! policy's `initial` and doubles up to its `max`; it returns to `initial` after
//! a connection that was established. The commands given while the connection is
//! down wait in the command channel for the next connection.
//!
//! Every channel is bounded. A caller waits when the command channel is full, and
//! the supervisor waits when the event channel is full: back-pressure reaches the
//! application, and nothing is dropped silently.

use std::fmt;
use std::net::SocketAddr;
use std::time::Duration;

use igs_iec104_codec::elements::{Dco, Nva, Qcc, Qoi, Qos, Rco, Sco, ShortFloat, Sva};
use igs_iec104_codec::formats::Cp56Time2a;
use igs_iec104_codec::header::{CommonAddress, InformationObjectAddress};
use igs_iec104_link::{ConfigError, LinkConfig, Parameters, Role};
use tokio::sync::mpsc;

use crate::error::TransportError;
use crate::procedures::{self, ProcedureError};
use crate::transport::{connect, run_shared, Command, Delivery};

/// How the client waits between two connection attempts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReconnectPolicy {
    /// The first delay, and the delay after a connection was established.
    pub initial: Duration,
    /// The longest delay.
    pub max: Duration,
}

impl Default for ReconnectPolicy {
    fn default() -> Self {
        Self {
            initial: Duration::from_secs(1),
            max: Duration::from_secs(60),
        }
    }
}

/// The configuration of a client: the controlled station, its common address, the
/// session parameters (the role is always controlling), the reconnect policy and
/// the capacity of the channels.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClientConfig {
    /// The address of the controlled station (port 2404 by default, §5.4).
    pub address: SocketAddr,
    /// The common address of the station's ASDUs.
    pub common_address: CommonAddress,
    /// How the client reconnects.
    pub reconnect: ReconnectPolicy,
    /// The number of items each channel holds before its sender waits.
    pub capacity: usize,
    link: LinkConfig,
}

impl ClientConfig {
    /// A client for the station at `address`, with the defaults of §9.6.
    pub fn new(address: SocketAddr, common_address: CommonAddress) -> Self {
        Self {
            address,
            common_address,
            reconnect: ReconnectPolicy::default(),
            capacity: 32,
            link: LinkConfig::with_defaults(Role::Controlling),
        }
    }

    /// Replaces the session parameters, after their validation.
    pub fn with_parameters(mut self, parameters: Parameters) -> Result<Self, ConfigError> {
        self.link = LinkConfig::new(Role::Controlling, parameters)?;
        Ok(self)
    }

    /// The session parameters.
    pub fn parameters(&self) -> Parameters {
        self.link.parameters()
    }
}

/// What the client tells the application.
#[derive(Debug)]
pub enum Event {
    /// A connection is established. The data transfer is stopped until it is started.
    Connected,
    /// The connection ended with this error. The client reconnects after the delay.
    Disconnected(TransportError),
    /// A connection attempt failed. The client tries again after the delay.
    ConnectFailed(TransportError),
    /// A delivery of the session: an ASDU, a rejected command, or a change of the
    /// state of the data transfer.
    Delivery(Delivery),
    /// The client has stopped: it was dropped. No event follows.
    Closed,
}

/// Why a request could not be made.
#[derive(Debug)]
pub enum ClientError {
    /// The client has stopped, so the request cannot be queued.
    Stopped,
    /// The request cannot be built.
    Procedure(ProcedureError),
}

impl fmt::Display for ClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Stopped => write!(f, "the client has stopped"),
            Self::Procedure(error) => write!(f, "the request cannot be built: {error}"),
        }
    }
}

impl std::error::Error for ClientError {}

impl From<ProcedureError> for ClientError {
    fn from(error: ProcedureError) -> Self {
        Self::Procedure(error)
    }
}

/// A client of one controlled station. Dropping it stops the supervisor.
#[derive(Debug)]
pub struct Client {
    commands: mpsc::Sender<Command>,
    events: mpsc::Receiver<Event>,
    common_address: CommonAddress,
}

impl Client {
    /// Starts the client. A supervisor task connects to the station and keeps the
    /// connection up. Needs a tokio runtime.
    pub fn connect(config: ClientConfig) -> Self {
        let (commands, commands_rx) = mpsc::channel(config.capacity);
        let (events_tx, events) = mpsc::channel(config.capacity);
        let common_address = config.common_address;
        tokio::spawn(supervise(config, commands_rx, events_tx));
        Self {
            commands,
            events,
            common_address,
        }
    }

    /// The next event; `None` once the client has stopped.
    pub async fn next_event(&mut self) -> Option<Event> {
        self.events.recv().await
    }

    /// Starts the data transfer (STARTDT act, §5.3).
    pub async fn start_data_transfer(&self) -> Result<(), ClientError> {
        self.send(Command::StartDt).await
    }

    /// Stops the data transfer (STOPDT act, §5.3).
    pub async fn stop_data_transfer(&self) -> Result<(), ClientError> {
        self.send(Command::StopDt).await
    }

    /// Sends an ASDU as it is. The procedures below build the usual ones.
    pub async fn send_asdu(&self, asdu: igs_iec104_codec::asdu::Asdu) -> Result<(), ClientError> {
        self.send(Command::SendAsdu(asdu)).await
    }

    /// General interrogation of `group` (§7.5).
    pub async fn interrogate(&self, group: Qoi) -> Result<(), ClientError> {
        self.request(procedures::interrogation(self.common_address, group))
            .await
    }

    /// Counter interrogation (§7.8).
    pub async fn counter_interrogation(&self, request: Qcc) -> Result<(), ClientError> {
        self.request(procedures::counter_interrogation(
            self.common_address,
            request,
        ))
        .await
    }

    /// Read of the object at `address` (§7.2).
    pub async fn read(&self, address: InformationObjectAddress) -> Result<(), ClientError> {
        self.request(procedures::read(self.common_address, address))
            .await
    }

    /// Clock synchronization with `time` (§7.6). Always an activation (D-004).
    pub async fn clock_sync(&self, time: Cp56Time2a) -> Result<(), ClientError> {
        self.request(procedures::clock_sync(self.common_address, time))
            .await
    }

    /// Test command with `pattern`, the time tag `time` (§7.10).
    pub async fn test_command(&self, pattern: u16, time: Cp56Time2a) -> Result<(), ClientError> {
        self.request(procedures::test_command(self.common_address, pattern, time))
            .await
    }

    /// Single command at `address`: select or execute, with a time tag or without (§7.7).
    pub async fn single_command(
        &self,
        address: InformationObjectAddress,
        command: Sco,
        time: Option<Cp56Time2a>,
    ) -> Result<(), ClientError> {
        self.request(procedures::single_command(
            self.common_address,
            address,
            command,
            time,
        ))
        .await
    }

    /// Double command at `address` (§7.7).
    pub async fn double_command(
        &self,
        address: InformationObjectAddress,
        command: Dco,
        time: Option<Cp56Time2a>,
    ) -> Result<(), ClientError> {
        self.request(procedures::double_command(
            self.common_address,
            address,
            command,
            time,
        ))
        .await
    }

    /// Regulating step command at `address` (§7.7).
    pub async fn regulating_step(
        &self,
        address: InformationObjectAddress,
        command: Rco,
        time: Option<Cp56Time2a>,
    ) -> Result<(), ClientError> {
        self.request(procedures::regulating_step(
            self.common_address,
            address,
            command,
            time,
        ))
        .await
    }

    /// Set-point command with a normalized value at `address` (§7.7).
    pub async fn set_point_normalized(
        &self,
        address: InformationObjectAddress,
        value: Nva,
        qualifier: Qos,
        time: Option<Cp56Time2a>,
    ) -> Result<(), ClientError> {
        self.request(procedures::set_point_normalized(
            self.common_address,
            address,
            value,
            qualifier,
            time,
        ))
        .await
    }

    /// Set-point command with a scaled value at `address` (§7.7).
    pub async fn set_point_scaled(
        &self,
        address: InformationObjectAddress,
        value: Sva,
        qualifier: Qos,
        time: Option<Cp56Time2a>,
    ) -> Result<(), ClientError> {
        self.request(procedures::set_point_scaled(
            self.common_address,
            address,
            value,
            qualifier,
            time,
        ))
        .await
    }

    /// Set-point command with a short floating point value at `address` (§7.7).
    pub async fn set_point_float(
        &self,
        address: InformationObjectAddress,
        value: ShortFloat,
        qualifier: Qos,
        time: Option<Cp56Time2a>,
    ) -> Result<(), ClientError> {
        self.request(procedures::set_point_float(
            self.common_address,
            address,
            value,
            qualifier,
            time,
        ))
        .await
    }

    async fn send(&self, command: Command) -> Result<(), ClientError> {
        self.commands
            .send(command)
            .await
            .map_err(|_| ClientError::Stopped)
    }

    async fn request(
        &self,
        asdu: Result<igs_iec104_codec::asdu::Asdu, ProcedureError>,
    ) -> Result<(), ClientError> {
        self.send(Command::SendAsdu(asdu?)).await
    }
}

/// Keeps one connection up, for as long as the client exists.
async fn supervise(
    config: ClientConfig,
    mut commands: mpsc::Receiver<Command>,
    events: mpsc::Sender<Event>,
) {
    let link = config.link;
    let t0 = link.parameters().t0;
    let (deliveries, mut delivered) = mpsc::channel(config.capacity);
    let forward = events.clone();
    tokio::spawn(async move {
        while let Some(delivery) = delivered.recv().await {
            if forward.send(Event::Delivery(delivery)).await.is_err() {
                break;
            }
        }
    });

    let mut delay = config.reconnect.initial;
    loop {
        match connect(config.address, t0).await {
            Ok(stream) => {
                delay = config.reconnect.initial;
                if events.send(Event::Connected).await.is_err() {
                    break;
                }
                match run_shared(stream, link, &mut commands, &deliveries).await {
                    // The command channel or the event channel is gone: the client was dropped.
                    Ok(()) => break,
                    Err(error) => {
                        if events.send(Event::Disconnected(error)).await.is_err() {
                            break;
                        }
                    }
                }
            }
            Err(error) => {
                if events.send(Event::ConnectFailed(error)).await.is_err() {
                    break;
                }
            }
        }
        tokio::time::sleep(delay).await;
        if events.is_closed() {
            break;
        }
        delay = delay.saturating_mul(2).min(config.reconnect.max);
    }
    // The client may already be gone: the event is then not wanted.
    let _ = events.send(Event::Closed).await;
}
