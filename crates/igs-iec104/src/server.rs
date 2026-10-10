// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! The controlled station: a server that accepts connections from controlling stations and
//! answers them (IEC 60870-5-104 §7.2 and §7.5 to §7.10, §10).
//!
//! [`Server::bind`] listens on the configured address. The connections of one server form
//! a redundancy group (§10.2): they share one process image, and only one of them carries
//! data at a time, the one that received the last STARTDT act. Each accepted connection runs
//! one APCI session of the controlled role ([`crate::transport::run`]), and its ASDUs are
//! answered by a [`Responder`].
//!
//! The events of the image wait in its queue until the started connection takes them. When a
//! connection is started, the others that are not stopped are closed (§10.7). The ASDUs of a
//! closed connection that the peer did not acknowledge, and those it had not sent yet, are
//! sent again on the connection that takes over, before the newer events (§10.5, §10.6). So
//! an event raised while no connection is started, or while a switchover is in progress, is
//! not lost, unless the queue of the image overflows.

use std::collections::{BTreeMap, VecDeque};
use std::fmt;
use std::io;
use std::net::SocketAddr;
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use igs_iec104_codec::asdu::Asdu;
use igs_iec104_codec::elements::Coi;
use igs_iec104_codec::formats::Cp56Time2a;
use igs_iec104_codec::header::{CommonAddress, InformationObjectAddress};
use igs_iec104_link::{ConfigError, LinkConfig, Parameters, Rejection, Role, TransferState};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, Notify};
use tokio::time;

use crate::clock;
use crate::process_image::{PointValue, ProcessError, ProcessImage, Update};
use crate::tls::Secure;
use crate::transport::{run, secure_server, Command, Delivery, Transport};

pub mod respond;

pub use respond::{CommandValue, Handler, Operation, Refusal, Responder};

/// The capacity of the channels between a connection and its transport.
const CHANNEL: usize = 64;
/// How often a connection retries its outgoing ASDUs while the window of k is full.
const RETRY: Duration = Duration::from_millis(10);
/// The pause after a failed accept, so that a full descriptor table does not spin.
const ACCEPT_RETRY: Duration = Duration::from_millis(100);
/// The default time a selection waits for its execution.
const DEFAULT_SELECTION_TIMEOUT: Duration = Duration::from_secs(30);
/// The default number of spontaneous events the station queues.
const DEFAULT_EVENT_CAPACITY: usize = 1024;

/// The configuration of a controlled station.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerConfig {
    /// The address the station listens on. The standard port is 2404 (§5.4).
    pub bind: SocketAddr,
    /// The common address of the station. A request to the global address is answered
    /// with it.
    pub common_address: CommonAddress,
    /// The timeouts and window sizes of each connection (§9.6).
    pub parameters: Parameters,
    /// The cause of initialization that the end of initialization reports (0 to 127): 0 for
    /// a local power switch on, 1 for a local manual reset, 2 for a remote reset.
    pub initialization_cause: u8,
    /// How long a selection waits for its execution.
    pub selection_timeout: Duration,
    /// The number of spontaneous events the station queues while no connection takes them.
    pub event_capacity: NonZeroUsize,
    /// The TLS backend of the connections (task X1), or `None` for plain TCP.
    pub tls: Option<Secure>,
}

impl ServerConfig {
    /// Secures the connections with `backend` (task X1). The client certificate is required.
    #[must_use]
    pub fn with_tls(mut self, backend: Secure) -> Self {
        self.tls = Some(backend);
        self
    }

    /// A configuration that listens on `bind` for the station `common_address`, with the
    /// default parameters, a local power switch on, a 30 s selection time-out and room for
    /// 1024 spontaneous events.
    pub fn new(bind: SocketAddr, common_address: CommonAddress) -> Self {
        Self {
            bind,
            common_address,
            parameters: Parameters::default(),
            initialization_cause: 0,
            selection_timeout: DEFAULT_SELECTION_TIMEOUT,
            event_capacity: NonZeroUsize::new(DEFAULT_EVENT_CAPACITY).unwrap_or(NonZeroUsize::MIN),
            tls: None,
        }
    }
}

/// A refusal to start the station.
#[derive(Debug)]
pub enum ServerError {
    /// The parameters are out of their ranges.
    Config(ConfigError),
    /// The cause of initialization is not 0 to 127.
    Initialization(u8),
    /// The process image refused its setup.
    Process(ProcessError),
    /// The address cannot be bound.
    Io(io::Error),
}

impl fmt::Display for ServerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Config(error) => write!(f, "{error}"),
            Self::Initialization(cause) => {
                write!(f, "the cause of initialization {cause} is out of range")
            }
            Self::Process(error) => write!(f, "{error}"),
            Self::Io(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for ServerError {}

/// The redundancy group of the station: its connections, and the state they share.
struct Group {
    image: ProcessImage,
    members: BTreeMap<u64, Member>,
    /// The connection whose data transfer is started, if any.
    started: Option<u64>,
    /// ASDUs to send on the started connection before the events of the image: those of
    /// the connections closed by a switchover that the peer did not acknowledge, and those
    /// they had not sent (§10.5, §10.6).
    carry: VecDeque<Asdu>,
    next_id: u64,
}

/// A connection of the group, as the group knows it.
struct Member {
    transfer: TransferState,
    /// Tells the connection that another one started the data transfer (§10.7).
    superseded: Arc<Notify>,
}

impl Group {
    fn join(&mut self) -> (u64, Arc<Notify>) {
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        let superseded = Arc::new(Notify::new());
        self.members.insert(
            id,
            Member {
                transfer: TransferState::Stopped,
                superseded: Arc::clone(&superseded),
            },
        );
        (id, superseded)
    }

    /// Records the state of the data transfer of connection `id`. A connection that starts
    /// supersedes every other connection that is not stopped (§10.7).
    fn transfer(&mut self, id: u64, state: TransferState) {
        if let Some(member) = self.members.get_mut(&id) {
            member.transfer = state;
        }
        if state == TransferState::Started {
            self.started = Some(id);
            for (other, member) in &self.members {
                if *other != id && member.transfer != TransferState::Stopped {
                    member.superseded.notify_one();
                }
            }
        } else if self.started == Some(id) {
            self.started = None;
        }
    }

    /// Removes connection `id`, and keeps the ASDUs it leaves unsent for the connection that
    /// takes over.
    fn leave(&mut self, id: u64, leftover: impl IntoIterator<Item = Asdu>) {
        self.members.remove(&id);
        if self.started == Some(id) {
            self.started = None;
        }
        self.carry.extend(leftover);
    }
}

/// What the connections of the station share.
struct Process {
    group: Mutex<Group>,
    /// Signals the started connection that the image has new events.
    queued: Notify,
}

fn lock(group: &Mutex<Group>) -> MutexGuard<'_, Group> {
    // A panic in another task cannot leave the group half-changed: each change is one call.
    group.lock().unwrap_or_else(PoisonError::into_inner)
}

/// What every connection of the station needs.
struct Station<H> {
    process: Arc<Process>,
    handler: H,
    link: LinkConfig,
    common_address: CommonAddress,
    initialization: Coi,
    selection_timeout: Duration,
}

/// A controlled station, ready to run. Create it with [`Server::bind`], publish its points
/// through [`Server::handle`], then run it with [`Server::run`].
pub struct Server<H: Handler> {
    listener: TcpListener,
    station: Arc<Station<H>>,
    tls: Option<Secure>,
}

impl<H: Handler> Server<H> {
    /// Checks the configuration and binds the listening address.
    pub async fn bind(config: ServerConfig, handler: H) -> Result<Self, ServerError> {
        let link =
            LinkConfig::new(Role::Controlled, config.parameters).map_err(ServerError::Config)?;
        let initialization = Coi::new(config.initialization_cause, false)
            .ok_or(ServerError::Initialization(config.initialization_cause))?;
        let image = ProcessImage::new(config.event_capacity).map_err(ServerError::Process)?;
        let listener = TcpListener::bind(config.bind)
            .await
            .map_err(ServerError::Io)?;
        let process = Arc::new(Process {
            group: Mutex::new(Group {
                image,
                members: BTreeMap::new(),
                started: None,
                carry: VecDeque::new(),
                next_id: 0,
            }),
            queued: Notify::new(),
        });
        Ok(Self {
            listener,
            tls: config.tls,
            station: Arc::new(Station {
                process,
                handler,
                link,
                common_address: config.common_address,
                initialization,
                selection_timeout: config.selection_timeout,
            }),
        })
    }

    /// The address the station listens on; the port is the one bound, even when it was 0.
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.listener.local_addr()
    }

    /// A handle to publish the points of the station.
    pub fn handle(&self) -> ServerHandle {
        ServerHandle {
            common_address: self.station.common_address,
            process: Arc::clone(&self.station.process),
        }
    }

    /// Accepts connections until the future is dropped. Connections already open finish on
    /// their own when it is dropped.
    pub async fn run(self) {
        loop {
            match self.listener.accept().await {
                Ok((stream, _peer)) => match &self.tls {
                    None => {
                        tokio::spawn(serve(stream, Arc::clone(&self.station)));
                    }
                    Some(tls) => {
                        tokio::spawn(serve_secured(
                            tls.clone(),
                            stream,
                            Arc::clone(&self.station),
                        ));
                    }
                },
                Err(_) => time::sleep(ACCEPT_RETRY).await,
            }
        }
    }
}

/// Publishes the points of a station. Clones share the same image.
#[derive(Clone)]
pub struct ServerHandle {
    common_address: CommonAddress,
    process: Arc<Process>,
}

impl ServerHandle {
    /// Registers a point of the station with its initial value (see [`ProcessImage::add_point`]).
    pub fn add_point(
        &self,
        address: InformationObjectAddress,
        value: PointValue,
        stamped: bool,
    ) -> Result<(), ProcessError> {
        lock(&self.process.group)
            .image
            .add_point(self.common_address, address, value, stamped)
    }

    /// Puts a point in a group, 1 to 16, for the group interrogations.
    pub fn set_group(
        &self,
        address: InformationObjectAddress,
        group: u8,
    ) -> Result<(), ProcessError> {
        lock(&self.process.group)
            .image
            .set_group(self.common_address, address, group)
    }

    /// Sets the value of a point at `time`. A change is sent by the started connection.
    pub fn update(
        &self,
        address: InformationObjectAddress,
        value: PointValue,
        time: Cp56Time2a,
    ) -> Result<Update, ProcessError> {
        let result =
            lock(&self.process.group)
                .image
                .update(self.common_address, address, value, time);
        if let Ok(Update::Queued) = result {
            self.process.queued.notify_one();
        }
        result
    }

    /// The current value of a point.
    pub fn value(&self, address: InformationObjectAddress) -> Option<PointValue> {
        lock(&self.process.group)
            .image
            .value(self.common_address, address)
    }
}

/// Secures an accepted connection, then serves it. A failed handshake ends the connection; the
/// handshake raises its security events.
async fn serve_secured<H: Handler>(tls: Secure, stream: TcpStream, station: Arc<Station<H>>) {
    let t0 = station.link.parameters().t0;
    if let Ok(secured) = secure_server(&tls, stream, t0).await {
        serve(secured, station).await;
    }
}

/// Serves one connection until its transport ends, or until another connection supersedes it.
async fn serve<S: Transport + 'static, H: Handler>(stream: S, station: Arc<Station<H>>) {
    let (id, superseded) = lock(&station.process.group).join();
    let (commands_tx, commands_rx) = mpsc::channel::<Command>(CHANNEL);
    let (deliveries_tx, mut deliveries_rx) = mpsc::channel::<Delivery>(CHANNEL);
    let transport = tokio::spawn(run(stream, station.link, commands_rx, deliveries_tx));
    let mut connection = Connection {
        station: &station,
        id,
        responder: Responder::new(station.common_address, station.selection_timeout),
        pending: VecDeque::new(),
        returned: VecDeque::new(),
        unacknowledged: Vec::new(),
        started: false,
        initialized: false,
    };
    let mut commands = Some(commands_tx);
    loop {
        tokio::select! {
            delivery = deliveries_rx.recv() => match delivery {
                Some(delivery) => {
                    // A refusal is sent again on the retry timer, not at once: a window that
                    // is still full would refuse it again, in a loop with no time passing.
                    let refused = matches!(delivery, Delivery::Rejected(_));
                    if !connection.deliver(delivery) {
                        break;
                    }
                    if refused {
                        continue;
                    }
                }
                None => break,
            },
            () = station.process.queued.notified(), if connection.started => {}
            () = superseded.notified() => {
                // The session closes the connection as §10.7 describes, and hands back its
                // unacknowledged ASDUs. A full command channel is not waited for: the close
                // follows when the commands are dropped below.
                if let Some(sender) = commands.as_ref() {
                    let _ = sender.try_send(Command::Supersede);
                }
                break;
            }
            () = time::sleep(RETRY), if connection.started && connection.has_outgoing() => {}
        }
        connection.take_events();
        let Some(sender) = commands.as_ref() else {
            break;
        };
        if !connection.flush(sender) {
            break;
        }
    }
    // Closing the commands ends the transport: it hands back its unacknowledged ASDUs, then
    // stops. They are kept for the connection that takes over, with the unsent ASDUs.
    drop(commands.take());
    while let Some(delivery) = deliveries_rx.recv().await {
        connection.deliver(delivery);
    }
    let _ = transport.await;
    let mut leftover = connection.unacknowledged;
    leftover.extend(connection.returned);
    leftover.extend(connection.pending);
    lock(&station.process.group).leave(id, leftover);
    // The connection that takes over may be waiting for the events of the leftover ASDUs.
    station.process.queued.notify_one();
}

/// The state of one connection between the transport and the station.
struct Connection<'a, H> {
    station: &'a Station<H>,
    id: u64,
    responder: Responder,
    /// ASDUs to send, in order: the answers, the events and the ASDUs of earlier connections.
    pending: VecDeque<Asdu>,
    /// ASDUs the transport refused, in the order they were refused. They precede `pending`.
    returned: VecDeque<Asdu>,
    /// ASDUs sent on this connection and not acknowledged, handed back by the transport.
    unacknowledged: Vec<Asdu>,
    started: bool,
    initialized: bool,
}

impl<H: Handler> Connection<'_, H> {
    /// Handles one delivery of the transport. Returns false when the connection must close.
    fn deliver(&mut self, delivery: Delivery) -> bool {
        match delivery {
            Delivery::Asdu(asdu) => {
                let Ok(clock) = clock::now_utc() else {
                    // Without a time the station cannot answer a clock synchronization.
                    return false;
                };
                let image = lock(&self.station.process.group);
                let replies = self.responder.respond(
                    &self.station.handler,
                    &image.image,
                    &asdu,
                    std::time::Instant::now(),
                    clock,
                );
                drop(image);
                self.pending.extend(replies);
            }
            Delivery::Transfer(state) => {
                self.started = state == TransferState::Started;
                lock(&self.station.process.group).transfer(self.id, state);
                if self.started && !self.initialized {
                    self.initialized = true;
                    let end = respond::end_of_initialization(
                        self.station.common_address,
                        self.station.initialization,
                    );
                    self.pending.extend(end);
                }
            }
            Delivery::Unacknowledged(asdus) => self.unacknowledged.extend(asdus),
            // The window is full, or the transfer stopped meanwhile: the ASDU is kept, and
            // sent again in order before the pending ones.
            Delivery::Rejected(Rejection::WindowFull(asdu) | Rejection::TransferStopped(asdu)) => {
                self.returned.push_back(asdu);
            }
            // The request does not apply to a controlled station.
            Delivery::Rejected(_) => {}
        }
        true
    }

    /// While this connection is started, takes the ASDUs kept for a switchover and the
    /// events of the image into its pending queue, in that order.
    fn take_events(&mut self) {
        if !self.started {
            return;
        }
        let mut group = lock(&self.station.process.group);
        if group.started != Some(self.id) {
            return;
        }
        while let Some(asdu) = group.carry.pop_front() {
            self.pending.push_back(asdu);
        }
        while let Some(asdu) = group.image.pop_event() {
            self.pending.push_back(asdu);
        }
    }

    /// True while an ASDU waits to be sent: a pending one, or one the transport refused.
    fn has_outgoing(&self) -> bool {
        !self.pending.is_empty() || !self.returned.is_empty()
    }

    /// Sends the pending ASDUs while the transport accepts them. Returns false when the
    /// transport has gone.
    fn flush(&mut self, commands: &mpsc::Sender<Command>) -> bool {
        use mpsc::error::TrySendError;
        if !self.started {
            return true;
        }
        // The refused ASDUs were sent before the pending ones: they go first, in their order.
        while let Some(asdu) = self.returned.pop_back() {
            self.pending.push_front(asdu);
        }
        while let Some(asdu) = self.pending.pop_front() {
            match commands.try_send(Command::SendAsdu(asdu)) {
                Ok(()) => {}
                Err(TrySendError::Full(Command::SendAsdu(asdu))) => {
                    self.pending.push_front(asdu);
                    return true;
                }
                Err(TrySendError::Full(_)) => return true,
                Err(TrySendError::Closed(_)) => return false,
            }
        }
        true
    }
}

#[cfg(test)]
mod tests;
