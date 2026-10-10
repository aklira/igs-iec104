// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! The controlling station with redundant connections (IEC 60870-5-104 §10, task S3).
//!
//! A [`RedundantClient`] keeps one connection to each endpoint of a redundancy group open
//! (§10.2). Only one of them carries data at a time: the started one. The group starts the
//! first endpoint that connects, and asks it for a station interrogation (§10.3, figure 32).
//! When the started connection is lost, it starts another connected endpoint (§10.5, §10.7),
//! and asks that one for a station interrogation too, because data may have been lost in the
//! switchover (§10.5). The unacknowledged ASDUs of the lost
//! connection are handed to the application, which decides whether to send them again: the
//! standard leaves the retransmission of commands to it (§10.5).
//!
//! Each endpoint is a [`Client`] with its own reconnection policy, so a lost connection is
//! reopened in the background. A switchover can also be asked for by the application
//! ([`RedundantClient::switch_to`]): the started connection is stopped, and the chosen one
//! started, as §10.2 describes.

use std::fmt;

use igs_iec104_codec::asdu::Asdu;
use igs_iec104_codec::elements::Qoi;
use igs_iec104_codec::header::CommonAddress;
use igs_iec104_link::TransferState;
use tokio::sync::{mpsc, oneshot};

use crate::client::{Client, ClientConfig, Event};
use crate::procedures::{self, ProcedureError};
use crate::transport::Delivery;

/// The capacity of the channel from the endpoints to the group, and of the commands.
const CHANNEL: usize = 64;

/// An event of the group: the event of one endpoint, or a change of the started connection.
#[derive(Debug)]
pub enum RedundancyEvent {
    /// An event of the endpoint at `index`, in the order of the endpoints given to
    /// [`RedundantClient::connect`].
    Endpoint {
        /// The index of the endpoint.
        index: usize,
        /// The event of the endpoint.
        event: Event,
    },
    /// The started connection changed. `from` is the connection that was started before,
    /// `None` at the first start. `to` is the connection that is started now.
    Switched {
        /// The connection started before.
        from: Option<usize>,
        /// The connection started now.
        to: usize,
    },
}

/// A request that the group cannot carry out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RedundancyError {
    /// The group has no endpoint.
    NoEndpoint,
    /// No endpoint is started, so there is no connection to send on.
    NoStartedConnection,
    /// The endpoint does not exist.
    UnknownEndpoint(usize),
    /// The endpoint is not connected, so it cannot be started.
    NotConnected(usize),
    /// The request cannot be built.
    Procedure(ProcedureError),
    /// The group has stopped.
    Stopped,
}

impl fmt::Display for RedundancyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoEndpoint => write!(f, "the redundancy group has no endpoint"),
            Self::NoStartedConnection => write!(f, "no connection of the group is started"),
            Self::UnknownEndpoint(index) => write!(f, "endpoint {index} does not exist"),
            Self::NotConnected(index) => write!(f, "endpoint {index} is not connected"),
            Self::Procedure(error) => write!(f, "{error}"),
            Self::Stopped => write!(f, "the redundancy group has stopped"),
        }
    }
}

impl std::error::Error for RedundancyError {}

/// A controlling station with a redundancy group of connections.
pub struct RedundantClient {
    commands: mpsc::Sender<Command>,
    events: mpsc::UnboundedReceiver<RedundancyEvent>,
}

impl RedundantClient {
    /// Opens a connection to each endpoint. The group starts the first one that connects.
    pub fn connect(endpoints: Vec<ClientConfig>) -> Result<Self, RedundancyError> {
        if endpoints.is_empty() {
            return Err(RedundancyError::NoEndpoint);
        }
        let (forward, from_endpoints) = mpsc::channel(CHANNEL);
        let (commands, commands_rx) = mpsc::channel(CHANNEL);
        let (events_tx, events) = mpsc::unbounded_channel();
        let mut group = Vec::with_capacity(endpoints.len());
        for (index, config) in endpoints.into_iter().enumerate() {
            let (requests, requests_rx) = mpsc::unbounded_channel();
            group.push(Endpoint {
                requests,
                common_address: config.common_address,
                connected: false,
            });
            let client = Client::connect(config);
            tokio::spawn(run_endpoint(index, client, requests_rx, forward.clone()));
        }
        tokio::spawn(run_group(group, from_endpoints, commands_rx, events_tx));
        Ok(Self { commands, events })
    }

    /// The next event of the group. `None` once the group has stopped.
    pub async fn next_event(&mut self) -> Option<RedundancyEvent> {
        self.events.recv().await
    }

    /// Sends an ASDU on the started connection.
    pub async fn send_asdu(&self, asdu: Asdu) -> Result<(), RedundancyError> {
        self.request(|reply| Command::Send { asdu, reply }).await
    }

    /// A general interrogation (§7.5) on the started connection, of the station or of a group.
    pub async fn interrogate(&self, group: Qoi) -> Result<(), RedundancyError> {
        self.request(|reply| Command::Interrogate { group, reply })
            .await
    }

    /// A manual switchover (§10.2): the started connection is stopped, and endpoint `index`
    /// is started. Nothing happens when it is the started one already.
    pub async fn switch_to(&self, index: usize) -> Result<(), RedundancyError> {
        self.request(|reply| Command::Switch { index, reply }).await
    }

    async fn request(
        &self,
        command: impl FnOnce(oneshot::Sender<Result<(), RedundancyError>>) -> Command,
    ) -> Result<(), RedundancyError> {
        let (reply, answer) = oneshot::channel();
        self.commands
            .send(command(reply))
            .await
            .map_err(|_| RedundancyError::Stopped)?;
        answer.await.map_err(|_| RedundancyError::Stopped)?
    }
}

/// What the group does with one endpoint.
enum Request {
    StartDt,
    StopDt,
    Send(Asdu),
}

enum Command {
    Send {
        asdu: Asdu,
        reply: oneshot::Sender<Result<(), RedundancyError>>,
    },
    Interrogate {
        group: Qoi,
        reply: oneshot::Sender<Result<(), RedundancyError>>,
    },
    Switch {
        index: usize,
        reply: oneshot::Sender<Result<(), RedundancyError>>,
    },
}

/// The group's view of one endpoint.
struct Endpoint {
    requests: mpsc::UnboundedSender<Request>,
    common_address: CommonAddress,
    connected: bool,
}

/// Runs one endpoint: its client, and the requests of the group to it.
async fn run_endpoint(
    index: usize,
    mut client: Client,
    mut requests: mpsc::UnboundedReceiver<Request>,
    forward: mpsc::Sender<(usize, Event)>,
) {
    loop {
        tokio::select! {
            event = client.next_event() => match event {
                Some(event) => {
                    if forward.send((index, event)).await.is_err() {
                        break;
                    }
                }
                None => break,
            },
            request = requests.recv() => match request {
                Some(Request::StartDt) => {
                    let _ = client.start_data_transfer().await;
                }
                Some(Request::StopDt) => {
                    let _ = client.stop_data_transfer().await;
                }
                Some(Request::Send(asdu)) => {
                    let _ = client.send_asdu(asdu).await;
                }
                None => break,
            },
        }
    }
}

/// The state of the group, and the handling of its events and commands.
struct State {
    endpoints: Vec<Endpoint>,
    /// The endpoint whose data transfer is started.
    started: Option<usize>,
    /// The endpoint whose STARTDT act was sent, and whose confirmation is awaited.
    starting: Option<usize>,
    /// The endpoint that was started last: the `from` of the next switchover.
    last_started: Option<usize>,
}

impl State {
    fn request(&self, index: usize, request: Request) -> Result<(), RedundancyError> {
        let endpoint = self
            .endpoints
            .get(index)
            .ok_or(RedundancyError::UnknownEndpoint(index))?;
        endpoint
            .requests
            .send(request)
            .map_err(|_| RedundancyError::Stopped)
    }

    /// Starts endpoint `index`, which must be connected and not started.
    fn start(&mut self, index: usize) {
        if self.request(index, Request::StartDt).is_ok() {
            self.starting = Some(index);
        }
    }

    /// Starts the first connected endpoint other than `excluded`, if there is one.
    fn start_another(&mut self, excluded: usize) {
        let next = self
            .endpoints
            .iter()
            .enumerate()
            .find(|(index, endpoint)| *index != excluded && endpoint.connected)
            .map(|(index, _)| index);
        if let Some(index) = next {
            self.start(index);
        }
    }

    /// Handles one event of endpoint `index`. Returns the events of the group it causes.
    fn endpoint_event(&mut self, index: usize, event: &Event) -> Option<RedundancyEvent> {
        match event {
            Event::Connected => {
                if let Some(endpoint) = self.endpoints.get_mut(index) {
                    endpoint.connected = true;
                }
                if self.started.is_none() && self.starting.is_none() {
                    self.start(index);
                }
            }
            Event::Disconnected(_) | Event::ConnectFailed(_) => {
                if let Some(endpoint) = self.endpoints.get_mut(index) {
                    endpoint.connected = false;
                }
                if self.started == Some(index) {
                    self.started = None;
                    self.start_another(index);
                } else if self.starting == Some(index) {
                    self.starting = None;
                    self.start_another(index);
                }
            }
            Event::Delivery(Delivery::Transfer(TransferState::Started))
                if self.starting == Some(index) =>
            {
                self.starting = None;
                let from = self.last_started;
                self.started = Some(index);
                self.last_started = Some(index);
                // A station interrogation follows every start: the first one (§10.3, figure 32)
                // and each switchover, so that no change is missed (§10.5).
                if let Some(endpoint) = self.endpoints.get(index) {
                    if let Ok(asdu) =
                        procedures::interrogation(endpoint.common_address, Qoi::STATION)
                    {
                        let _ = self.request(index, Request::Send(asdu));
                    }
                }
                return Some(RedundancyEvent::Switched { from, to: index });
            }
            _ => {}
        }
        None
    }

    fn command(&mut self, command: Command) {
        match command {
            Command::Send { asdu, reply } => {
                let result = match self.started {
                    Some(index) => self.request(index, Request::Send(asdu)),
                    None => Err(RedundancyError::NoStartedConnection),
                };
                let _ = reply.send(result);
            }
            Command::Interrogate { group, reply } => {
                let result = match self
                    .started
                    .and_then(|index| self.endpoints.get(index).map(|e| (index, e.common_address)))
                {
                    Some((index, common)) => match procedures::interrogation(common, group) {
                        Ok(asdu) => self.request(index, Request::Send(asdu)),
                        Err(error) => Err(RedundancyError::Procedure(error)),
                    },
                    None => Err(RedundancyError::NoStartedConnection),
                };
                let _ = reply.send(result);
            }
            Command::Switch { index, reply } => {
                let _ = reply.send(self.switch_to(index));
            }
        }
    }

    fn switch_to(&mut self, index: usize) -> Result<(), RedundancyError> {
        let endpoint = self
            .endpoints
            .get(index)
            .ok_or(RedundancyError::UnknownEndpoint(index))?;
        if self.started == Some(index) || self.starting == Some(index) {
            return Ok(());
        }
        if !endpoint.connected {
            return Err(RedundancyError::NotConnected(index));
        }
        if let Some(current) = self.started.take() {
            self.request(current, Request::StopDt)?;
            self.last_started = Some(current);
        }
        self.start(index);
        Ok(())
    }
}

/// The group task: the events of the endpoints, and the commands of the application.
async fn run_group(
    endpoints: Vec<Endpoint>,
    mut from_endpoints: mpsc::Receiver<(usize, Event)>,
    mut commands: mpsc::Receiver<Command>,
    events: mpsc::UnboundedSender<RedundancyEvent>,
) {
    let mut state = State {
        endpoints,
        started: None,
        starting: None,
        last_started: None,
    };
    loop {
        tokio::select! {
            item = from_endpoints.recv() => {
                let Some((index, event)) = item else {
                    break;
                };
                if let Some(change) = state.endpoint_event(index, &event) {
                    let _ = events.send(change);
                }
                let _ = events.send(RedundancyEvent::Endpoint { index, event });
            }
            command = commands.recv() => {
                let Some(command) = command else {
                    break;
                };
                state.command(command);
            }
        }
    }
}

#[cfg(test)]
mod tests;
