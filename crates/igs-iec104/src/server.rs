// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! The controlled station: a server that accepts connections from controlling stations and
//! answers them (IEC 60870-5-104 §7.2 and §7.5 to §7.10, task S2).
//!
//! [`Server::bind`] listens on the configured address. Each accepted connection runs one
//! APCI session of the controlled role ([`crate::transport::run`]), and its ASDUs are answered by a
//! [`Responder`]. The points of the station are published through a [`ServerHandle`]. An
//! update that changes a point is queued in the [`ProcessImage`]; the dispatcher then sends
//! it to every connection whose data transfer has started, as a spontaneous ASDU.
//!
//! A connection that falls behind the spontaneous events is closed rather than given an
//! incomplete stream: the controlling station recovers the state with an interrogation.

use std::collections::VecDeque;
use std::fmt;
use std::io;
use std::net::SocketAddr;
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use igs_iec104_codec::asdu::Asdu;
use igs_iec104_codec::elements::Coi;
use igs_iec104_codec::formats::Cp56Time2a;
use igs_iec104_codec::header::{CommonAddress, InformationObjectAddress};
use igs_iec104_link::{ConfigError, LinkConfig, Parameters, Rejection, Role, TransferState};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, mpsc, Notify};
use tokio::task::JoinHandle;
use tokio::time;

use crate::clock;
use crate::process_image::{PointValue, ProcessError, ProcessImage, Update};
use crate::transport::{run, Command, Delivery};

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
    /// The number of spontaneous events the station queues, and of events a connection may
    /// lag behind before it is closed.
    pub event_capacity: NonZeroUsize,
}

impl ServerConfig {
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

/// What the process image and the dispatcher share.
struct Process {
    image: Mutex<ProcessImage>,
    queued: Notify,
    events: broadcast::Sender<Asdu>,
}

fn lock(image: &Mutex<ProcessImage>) -> MutexGuard<'_, ProcessImage> {
    // A panic in another task cannot leave the image half-changed: each change is one call.
    image.lock().unwrap_or_else(PoisonError::into_inner)
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
}

impl<H: Handler> Server<H> {
    /// Checks the configuration and binds the listening address.
    pub async fn bind(config: ServerConfig, handler: H) -> Result<Self, ServerError> {
        let link =
            LinkConfig::new(Role::Controlled, config.parameters).map_err(ServerError::Config)?;
        let initialization = Coi::new(config.initialization_cause, false)
            .ok_or(ServerError::Initialization(config.initialization_cause))?;
        let image = ProcessImage::new(config.event_capacity).map_err(ServerError::Process)?;
        let (events, _) = broadcast::channel(config.event_capacity.get());
        let listener = TcpListener::bind(config.bind)
            .await
            .map_err(ServerError::Io)?;
        let process = Arc::new(Process {
            image: Mutex::new(image),
            queued: Notify::new(),
            events,
        });
        Ok(Self {
            listener,
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

    /// Accepts connections and dispatches the spontaneous events, until the future is
    /// dropped. Each connection is served on its own task: connections already open
    /// finish on their own when the future is dropped.
    pub async fn run(self) {
        let process = Arc::clone(&self.station.process);
        tokio::select! {
            () = dispatch(process) => {}
            () = self.accept_connections() => {}
        }
    }

    async fn accept_connections(&self) {
        loop {
            match self.listener.accept().await {
                Ok((stream, _peer)) => {
                    tokio::spawn(serve(stream, Arc::clone(&self.station)));
                }
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
        lock(&self.process.image).add_point(self.common_address, address, value, stamped)
    }

    /// Puts a point in a group, 1 to 16, for the group interrogations.
    pub fn set_group(
        &self,
        address: InformationObjectAddress,
        group: u8,
    ) -> Result<(), ProcessError> {
        lock(&self.process.image).set_group(self.common_address, address, group)
    }

    /// Sets the value of a point at `time`. A change is sent to the started connections.
    pub fn update(
        &self,
        address: InformationObjectAddress,
        value: PointValue,
        time: Cp56Time2a,
    ) -> Result<Update, ProcessError> {
        let result = lock(&self.process.image).update(self.common_address, address, value, time);
        if let Ok(Update::Queued) = result {
            self.process.queued.notify_one();
        }
        result
    }

    /// The current value of a point.
    pub fn value(&self, address: InformationObjectAddress) -> Option<PointValue> {
        lock(&self.process.image).value(self.common_address, address)
    }
}

/// Moves the queued spontaneous events to the connections, in order.
async fn dispatch(process: Arc<Process>) {
    loop {
        process.queued.notified().await;
        let drained: Vec<Asdu> = {
            let mut image = lock(&process.image);
            std::iter::from_fn(|| image.pop_event()).collect()
        };
        for asdu in drained {
            // The send fails only when no connection is subscribed: nothing to deliver.
            let _ = process.events.send(asdu);
        }
    }
}

/// Serves one connection until its transport ends, or until it can no longer keep up.
async fn serve<H: Handler>(stream: TcpStream, station: Arc<Station<H>>) {
    let mut events = station.process.events.subscribe();
    let (commands_tx, commands_rx) = mpsc::channel::<Command>(CHANNEL);
    let (deliveries_tx, mut deliveries_rx) = mpsc::channel::<Delivery>(CHANNEL);
    let transport: JoinHandle<_> =
        tokio::spawn(run(stream, station.link, commands_rx, deliveries_tx));
    let mut connection = Connection {
        station: &*station,
        responder: Responder::new(station.common_address, station.selection_timeout),
        pending: VecDeque::new(),
        started: false,
        initialized: false,
    };
    loop {
        tokio::select! {
            delivery = deliveries_rx.recv() => match delivery {
                Some(delivery) => {
                    if !connection.deliver(delivery) {
                        break;
                    }
                }
                None => break,
            },
            event = events.recv() => match event {
                Ok(asdu) => connection.spontaneous(asdu),
                // Lagged: the connection missed events. Closed: the station is gone.
                Err(_) => break,
            },
            () = time::sleep(RETRY), if !connection.pending.is_empty() => {}
        }
        if !connection.flush(&commands_tx) {
            break;
        }
    }
    // Closing both channels ends the transport: its loop stops on either.
    drop(deliveries_rx);
    drop(commands_tx);
    let _ = transport.await;
}

/// The state of one connection between the transport and the station.
struct Connection<'a, H> {
    station: &'a Station<H>,
    responder: Responder,
    /// ASDUs waiting for the window of k: the answers and the spontaneous events, in order.
    pending: VecDeque<Asdu>,
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
                let image = lock(&self.station.process.image);
                let replies = self.responder.respond(
                    &self.station.handler,
                    &image,
                    &asdu,
                    Instant::now(),
                    clock,
                );
                drop(image);
                self.pending.extend(replies);
            }
            Delivery::Transfer(state) => {
                self.started = state == TransferState::Started;
                if self.started && !self.initialized {
                    self.initialized = true;
                    let end = respond::end_of_initialization(
                        self.station.common_address,
                        self.station.initialization,
                    );
                    self.pending.extend(end);
                }
            }
            Delivery::Rejected(Rejection::WindowFull(asdu)) => {
                // Sent again later, before the ASDUs that followed it.
                self.pending.push_front(asdu);
            }
            // The transfer stopped meanwhile, or the request does not apply to a controlled
            // station: the ASDU is dropped.
            Delivery::Rejected(_) => {}
        }
        true
    }

    /// Queues a spontaneous event of this station, while its data transfer is started.
    fn spontaneous(&mut self, asdu: Asdu) {
        if self.started && asdu.common_address == self.station.common_address {
            self.pending.push_back(asdu);
        }
    }

    /// Sends the pending ASDUs while the transport accepts them. Returns false when the
    /// transport has gone.
    fn flush(&mut self, commands: &mpsc::Sender<Command>) -> bool {
        use mpsc::error::TrySendError;
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
