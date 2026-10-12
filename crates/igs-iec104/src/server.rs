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
//! connection is started, the others that are not stopped are closed (§10.7). The events of a
//! closed connection that the peer did not acknowledge, and those it had not sent yet, are
//! sent again on the connection that takes over, before the newer events (§10.5, §10.6). So
//! an event raised while no connection is started, or while a switchover is in progress, is
//! not lost, unless the queue of the image overflows. The answers of a closed connection are
//! not sent again: they belong to the requests of that connection (D-021).

use std::collections::{BTreeMap, VecDeque};
use std::fmt;
use std::future::Future;
use std::io;
use std::net::SocketAddr;
use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use igs_iec104_codec::asdu::Asdu;
use igs_iec104_codec::elements::Coi;
use igs_iec104_codec::formats::Cp56Time2a;
use igs_iec104_codec::header::{cause, CommonAddress, InformationObjectAddress};
use igs_iec104_link::{ConfigError, LinkConfig, Parameters, Rejection, Role, TransferState};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, watch, Notify};
use tokio::task::{JoinHandle, JoinSet};
use tokio::time;

use crate::clock;
use crate::error::TransportError;
use crate::process_image::{PointValue, ProcessError, ProcessImage, Update};
use crate::tls::Secure;
use crate::transport::{run, secure_server, without_delay, Command, Delivery, Transport};

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
    /// The capacity of the stream of connection events (task U2), or `None` for no stream.
    pub events: Option<NonZeroUsize>,
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
            events: None,
        }
    }

    /// Reports the connections of the station on a stream of `capacity` events, taken with
    /// [`Server::events`] (task U2).
    #[must_use]
    pub fn with_events(mut self, capacity: NonZeroUsize) -> Self {
        self.events = Some(capacity);
        self
    }
}

/// What the station reports about its connections (task U2). The identifier of a connection is
/// the same in every event about it.
#[derive(Debug)]
pub enum ServerEvent {
    /// A connection was accepted, after its TLS handshake when it is secured.
    Connected {
        /// The identifier of the connection.
        id: u64,
        /// The address of the controlling station.
        peer: SocketAddr,
    },
    /// The data transfer of a connection changed state (STARTDT, STOPDT, §5.3).
    Transfer {
        /// The identifier of the connection.
        id: u64,
        /// The new state.
        state: TransferState,
    },
    /// Another connection started its data transfer, so this one, which was not stopped, is
    /// closed (§10.7).
    Superseded {
        /// The identifier of the connection that is closed.
        id: u64,
        /// The identifier of the connection that started.
        by: u64,
    },
    /// A connection ended.
    Closed {
        /// The identifier of the connection.
        id: u64,
        /// Why it ended.
        end: ConnectionEnd,
    },
}

/// Why a connection of the station ended (task U2).
#[derive(Debug)]
pub enum ConnectionEnd {
    /// The station closed it: the station stopped, or the connection was superseded.
    Station,
    /// The transport ended with this error: the peer closed the stream, the link closed the
    /// connection (a time-out, for example), or the stream failed.
    Transport(TransportError),
    /// The station stopped and closed it by force at the end of the grace period.
    Forced,
}

/// The stream of the events of a station, taken once with [`Server::events`] (task U2). When it
/// is full, the station does not wait: the event is dropped and counted.
#[derive(Debug)]
pub struct ServerEvents {
    events: mpsc::Receiver<ServerEvent>,
    dropped: Arc<AtomicU64>,
}

impl ServerEvents {
    /// The next event; `None` once the station has stopped and every event was read.
    pub async fn next(&mut self) -> Option<ServerEvent> {
        self.events.recv().await
    }

    /// The number of events the station dropped because the stream was full.
    pub fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }
}

/// The station's end of the event stream. It never waits.
#[derive(Clone, Debug)]
struct EventSink {
    events: mpsc::Sender<ServerEvent>,
    dropped: Arc<AtomicU64>,
}

impl EventSink {
    fn emit(&self, event: ServerEvent) {
        if self.events.try_send(event).is_err() {
            self.dropped.fetch_add(1, Ordering::Relaxed);
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
    /// Events to send on the started connection before the events of the image: those of
    /// the connections closed by a switchover that the peer did not acknowledge, and those
    /// they had not sent (§10.5, §10.6). The answers of a closed connection are not kept (D-021).
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
    /// Returns the connections it supersedes.
    fn transfer(&mut self, id: u64, state: TransferState) -> Vec<u64> {
        if let Some(member) = self.members.get_mut(&id) {
            member.transfer = state;
        }
        let mut superseded = Vec::new();
        if state == TransferState::Started {
            self.started = Some(id);
            for (other, member) in &self.members {
                if *other != id && member.transfer != TransferState::Stopped {
                    member.superseded.notify_one();
                    superseded.push(*other);
                }
            }
        } else if self.started == Some(id) {
            self.started = None;
        }
        superseded
    }

    /// Removes connection `id`, and keeps the events it leaves unsent for the connection that
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
    /// Set to true when the station stops (task U1): each connection sends what it has, then
    /// closes.
    stop: watch::Sender<bool>,
    /// The stream of connection events, when the configuration asks for it (task U2).
    events: Option<EventSink>,
}

impl<H> Station<H> {
    fn report(&self, event: ServerEvent) {
        if let Some(events) = &self.events {
            events.emit(event);
        }
    }
}

/// What a stop of the station did (task U1).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ShutdownReport {
    /// The connections open when the stop began.
    pub connections: usize,
    /// The connections still open when the grace period ended, closed by force.
    pub forced: usize,
    /// The events the station still held when it stopped: not sent, or sent and not acknowledged
    /// by the controlling station.
    pub pending_events: usize,
}

/// A task that is aborted when its handle is dropped, so that a connection stopped by force
/// closes its transport too.
struct AbortOnDrop<T>(JoinHandle<T>);

impl<T> Drop for AbortOnDrop<T> {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// A controlled station, ready to run. Create it with [`Server::bind`], publish its points
/// through [`Server::handle`], then run it with [`Server::run`], or with [`Server::run_until`] to
/// stop it in order.
///
/// # Examples
///
/// A controlled station with one single point, which is on. It refuses every command, and answers the
/// interrogation of its controlling station:
///
/// ```no_run
/// use igs_iec104::process_image::PointValue;
/// use igs_iec104::server::{Handler, Operation, Refusal, Server, ServerConfig};
/// use igs_iec104_codec::elements::{QualityFlags, Siq};
/// use igs_iec104_codec::header::{CommonAddress, InformationObjectAddress};
///
/// struct Refuse;
///
/// impl Handler for Refuse {
///     fn select(&self, _operation: &Operation) -> Result<(), Refusal> {
///         Err(Refusal::Rejected)
///     }
///
///     fn execute(&self, _operation: &Operation) -> Result<(), Refusal> {
///         Err(Refusal::Rejected)
///     }
/// }
///
/// # #[tokio::main(flavor = "current_thread")]
/// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let config = ServerConfig::new("127.0.0.1:2404".parse()?, CommonAddress::new(1));
/// let server = Server::bind(config, Refuse).await?;
/// server.handle().add_point(
///     InformationObjectAddress::new(1).ok_or("address 1")?,
///     PointValue::Single(Siq {
///         on: true,
///         quality: QualityFlags::default(),
///     }),
///     false,
/// )?;
/// server.run().await;
/// # Ok(())
/// # }
/// ```
pub struct Server<H: Handler> {
    listener: TcpListener,
    station: Arc<Station<H>>,
    tls: Option<Secure>,
    events: Option<ServerEvents>,
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
        let (sink, events) = match config.events {
            Some(capacity) => {
                let (sender, receiver) = mpsc::channel(capacity.get());
                let dropped = Arc::new(AtomicU64::new(0));
                (
                    Some(EventSink {
                        events: sender,
                        dropped: Arc::clone(&dropped),
                    }),
                    Some(ServerEvents {
                        events: receiver,
                        dropped,
                    }),
                )
            }
            None => (None, None),
        };
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
            events,
            station: Arc::new(Station {
                process,
                handler,
                link,
                common_address: config.common_address,
                initialization,
                selection_timeout: config.selection_timeout,
                stop: watch::Sender::new(false),
                events: sink,
            }),
        })
    }

    /// The address the station listens on; the port is the one bound, even when it was 0.
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.listener.local_addr()
    }

    /// The stream of the station's connection events, once: `None` when it was already taken, or
    /// when the configuration did not ask for it ([`ServerConfig::with_events`]).
    pub fn events(&mut self) -> Option<ServerEvents> {
        self.events.take()
    }

    /// A handle to publish the points of the station.
    pub fn handle(&self) -> ServerHandle {
        ServerHandle {
            common_address: self.station.common_address,
            process: Arc::clone(&self.station.process),
        }
    }

    /// Accepts connections and serves them, until the future is dropped. Dropping it closes the
    /// connections at once; [`Server::run_until`] stops them in order.
    pub async fn run(self) {
        let _ = self
            .run_until(std::future::pending::<()>(), Duration::ZERO)
            .await;
    }

    /// Accepts connections and serves them until `stop` completes, then stops the station (task
    /// U1): it accepts no more connections, each connection sends the ASDUs it has already taken
    /// and closes, and the connections still open after `grace` are closed by force. Returns what
    /// the stop did. The station sends no STOPDT: that request belongs to the controlling station.
    pub async fn run_until(
        self,
        stop: impl Future<Output = ()>,
        grace: Duration,
    ) -> ShutdownReport {
        let mut connections = JoinSet::new();
        tokio::pin!(stop);
        loop {
            tokio::select! {
                () = &mut stop => break,
                accepted = self.listener.accept() => match accepted {
                    Ok((stream, peer)) => {
                        // A connection that cannot be set up this way is served all the same, with
                        // the delay of the algorithm (transport::without_delay).
                        let _ = without_delay(&stream);
                        match &self.tls {
                            None => {
                                connections.spawn(serve(stream, Arc::clone(&self.station), peer));
                            }
                            Some(tls) => {
                                connections.spawn(serve_secured(
                                    tls.clone(),
                                    stream,
                                    Arc::clone(&self.station),
                                    peer,
                                ));
                            }
                        }
                    },
                    Err(_) => time::sleep(ACCEPT_RETRY).await,
                },
                // The connections that end are reaped, so that the set holds the open ones.
                Some(_) = connections.join_next(), if !connections.is_empty() => {}
            }
        }
        drop(self.listener);
        while connections.try_join_next().is_some() {}
        let open = connections.len();
        self.station.stop.send_replace(true);
        let forced = close_connections(&mut connections, grace).await;
        let group = lock(&self.station.process.group);
        ShutdownReport {
            connections: open,
            forced,
            pending_events: group.carry.len().saturating_add(group.image.pending()),
        }
    }
}

/// Waits up to `grace` for the connections to close, then closes the others by force. Returns the
/// number closed by force. The set is empty on return.
async fn close_connections(connections: &mut JoinSet<()>, grace: Duration) -> usize {
    let _ = time::timeout(grace, async {
        while connections.join_next().await.is_some() {}
    })
    .await;
    while connections.try_join_next().is_some() {}
    let forced = connections.len();
    connections.abort_all();
    while connections.join_next().await.is_some() {}
    forced
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
async fn serve_secured<H: Handler>(
    tls: Secure,
    stream: TcpStream,
    station: Arc<Station<H>>,
    peer: SocketAddr,
) {
    let t0 = station.link.parameters().t0;
    let mut stopping = station.stop.subscribe();
    let secured = tokio::select! {
        secured = secure_server(&tls, stream, t0) => secured,
        // The station stops during the handshake: the connection is not served.
        _ = stopping.wait_for(|stop| *stop) => return,
    };
    if let Ok(secured) = secured {
        serve(secured, station, peer).await;
    }
}

/// Serves one connection until its transport ends, or until another connection supersedes it.
async fn serve<S: Transport + 'static, H: Handler>(
    stream: S,
    station: Arc<Station<H>>,
    peer: SocketAddr,
) {
    let (id, superseded) = lock(&station.process.group).join();
    station.report(ServerEvent::Connected { id, peer });
    // Reports the end of the connection when it is dropped: at the end of this function, or when
    // the station closes the connection by force.
    let mut closed = ClosedReport {
        events: station.events.clone(),
        id,
        end: None,
    };
    let (commands_tx, commands_rx) = mpsc::channel::<Command>(CHANNEL);
    let (deliveries_tx, mut deliveries_rx) = mpsc::channel::<Delivery>(CHANNEL);
    let mut transport = AbortOnDrop(tokio::spawn(run(
        stream,
        station.link,
        commands_rx,
        deliveries_tx,
    )));
    let mut stopping = station.stop.subscribe();
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
            _ = stopping.wait_for(|stop| *stop) => {
                // The station stops (task U1): the ASDUs already taken are handed to the
                // transport, no new event is taken, and closing the commands below closes the
                // connection once the transport has sent them.
                if let Some(sender) = commands.as_ref() {
                    connection.flush(sender);
                }
                break;
            }
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
    closed.end = Some(match (&mut transport.0).await {
        Ok(Err(error)) => ConnectionEnd::Transport(error),
        Ok(Ok(())) | Err(_) => ConnectionEnd::Station,
    });
    let mut leftover = connection.unacknowledged;
    leftover.extend(connection.returned);
    leftover.extend(connection.pending);
    // The image queues its events with cause 3 (spontaneous). The answers of this connection
    // belong to its requests, and its end of initialization to its start: neither is sent
    // again on the connection that takes over (D-021).
    leftover.retain(|asdu| asdu.cot.cause() == cause::SPONTANEOUS);
    lock(&station.process.group).leave(id, leftover);
    // The connection that takes over may be waiting for the events of the leftover ASDUs.
    station.process.queued.notify_one();
}

/// Reports the end of a connection when it is dropped. Without an end set, the connection was
/// closed by force.
struct ClosedReport {
    events: Option<EventSink>,
    id: u64,
    end: Option<ConnectionEnd>,
}

impl Drop for ClosedReport {
    fn drop(&mut self) {
        if let Some(events) = &self.events {
            events.emit(ServerEvent::Closed {
                id: self.id,
                end: self.end.take().unwrap_or(ConnectionEnd::Forced),
            });
        }
    }
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
                let superseded = lock(&self.station.process.group).transfer(self.id, state);
                self.station
                    .report(ServerEvent::Transfer { id: self.id, state });
                for other in superseded {
                    self.station.report(ServerEvent::Superseded {
                        id: other,
                        by: self.id,
                    });
                }
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
            // A controlled session does not report its transmissions.
            Delivery::Transmitted(_) => {}
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
