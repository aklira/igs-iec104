// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! The driver: one APCI session over a byte stream (IEC 60870-5-104 §5.4, §9.6).
//!
//! The session of `igs-iec104-link` decides what happens; this module does the
//! I/O. [`run`] reads the stream and feeds the session with the bytes and the
//! instant, writes the frames the session returns, passes the ASDUs to the
//! application, and shuts the stream down when the connection ends. [`connect`]
//! opens the TCP connection within t0.
//!
//! The stream is any [`Transport`], so TLS (task X1) plugs in without changing
//! the driver. Time is read from tokio's clock, so `tokio::time::pause` drives
//! every timer in tests.

use std::future::Future;
use std::io;
use std::net::SocketAddr;
use std::time::{Duration, Instant};

use igs_iec104_codec::asdu::Asdu;
use igs_iec104_link::{Action, Event, LinkConfig, Rejection, Session, TransferState};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::time::{self, Instant as TokioInstant};

use crate::error::TransportError;

/// The TCP port of IEC 60870-5-104, confirmed by IANA. The controlled station
/// listens on it; the controlling station may use any port (§5.4).
pub const DEFAULT_PORT: u16 = 2404;

/// A byte stream that the driver runs over: TCP now, TLS later (task X1).
pub trait Transport: AsyncRead + AsyncWrite + Unpin + Send {}

impl<T> Transport for T where T: AsyncRead + AsyncWrite + Unpin + Send {}

/// A request of the application to the connection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    /// Send this ASDU as an I frame.
    SendAsdu(Asdu),
    /// Start the data transfer (controlling station).
    StartDt,
    /// Stop the data transfer (controlling station).
    StopDt,
}

/// What the connection tells the application.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Delivery {
    /// An ASDU received from the peer.
    Asdu(Asdu),
    /// A command that was not carried out, with the ASDU when there is one.
    Rejected(Rejection),
    /// The state of the data transfer changed: `Started` once STARTDT con is received,
    /// `Stopped` once STOPDT con is received (figures 17 and 18).
    Transfer(TransferState),
}

/// Opens a TCP connection to `address`, giving up after `t0` (§9.6).
pub async fn connect(address: SocketAddr, t0: Duration) -> Result<TcpStream, TransportError> {
    within_t0(t0, TcpStream::connect(address)).await
}

/// Waits for `connection` for at most `t0`.
async fn within_t0<T, F>(t0: Duration, connection: F) -> Result<T, TransportError>
where
    F: Future<Output = io::Result<T>>,
{
    match time::timeout(t0, connection).await {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(error)) => Err(TransportError::Connect(error)),
        Err(_elapsed) => Err(TransportError::ConnectTimeout),
    }
}

/// Runs one connection until it ends.
///
/// The connection ends in three ways. When the application drops the command
/// channel or the delivery channel, it ends with `Ok`. When the session closes
/// the connection, the error is [`TransportError::Closed`]. When the peer closes
/// the stream, or the stream fails, the error says so. In every case the stream
/// is shut down before returning.
pub async fn run<S: Transport>(
    stream: S,
    config: LinkConfig,
    mut commands: mpsc::Receiver<Command>,
    deliveries: mpsc::Sender<Delivery>,
) -> Result<(), TransportError> {
    run_shared(stream, config, &mut commands, &deliveries).await
}

/// Like [`run`], with the channels borrowed. A client keeps the same channels
/// across its connections, so that commands given while a connection is down
/// wait for the next one.
pub async fn run_shared<S: Transport>(
    mut stream: S,
    config: LinkConfig,
    commands: &mut mpsc::Receiver<Command>,
    deliveries: &mpsc::Sender<Delivery>,
) -> Result<(), TransportError> {
    let mut session = Session::new(config, now());
    let outcome = drive(&mut stream, &mut session, commands, deliveries).await;
    // Nothing is left to recover when the shutdown fails: the peer is gone.
    let _ = stream.shutdown().await;
    outcome
}

async fn drive<S: Transport>(
    stream: &mut S,
    session: &mut Session,
    commands: &mut mpsc::Receiver<Command>,
    deliveries: &mpsc::Sender<Delivery>,
) -> Result<(), TransportError> {
    let mut transfer = session.transfer();
    loop {
        let event = tokio::select! {
            read = read_chunk(stream) => match read.map_err(TransportError::Io)? {
                Some(bytes) => Event::Received(bytes),
                None => return Err(TransportError::PeerClosed),
            },
            command = commands.recv() => match command {
                Some(command) => event_of(command),
                None => return Ok(()),
            },
            () = wait_until(session.next_deadline()) => Event::Tick,
        };
        for action in session.handle(event, now()) {
            match action {
                Action::Send(apdu) => {
                    let bytes = apdu.to_vec().map_err(TransportError::Encode)?;
                    stream.write_all(&bytes).await.map_err(TransportError::Io)?;
                }
                Action::Deliver(asdu) => {
                    if deliveries.send(Delivery::Asdu(asdu)).await.is_err() {
                        return Ok(());
                    }
                }
                Action::Rejected(rejection) => {
                    if deliveries
                        .send(Delivery::Rejected(rejection))
                        .await
                        .is_err()
                    {
                        return Ok(());
                    }
                }
                Action::Close(reason) => return Err(TransportError::Closed(reason)),
            }
        }
        if session.transfer() != transfer {
            transfer = session.transfer();
            if deliveries.send(Delivery::Transfer(transfer)).await.is_err() {
                return Ok(());
            }
        }
    }
}

/// The bytes of the next read, or `None` when the stream has ended.
async fn read_chunk<S: AsyncRead + Unpin>(stream: &mut S) -> io::Result<Option<Vec<u8>>> {
    let mut chunk = Vec::new();
    let read = stream.read_buf(&mut chunk).await?;
    Ok((read > 0).then_some(chunk))
}

/// Completes at the session's next deadline, or never when no timer runs.
async fn wait_until(deadline: Option<Instant>) {
    match deadline {
        Some(at) => time::sleep_until(TokioInstant::from_std(at)).await,
        None => std::future::pending().await,
    }
}

fn event_of(command: Command) -> Event {
    match command {
        Command::SendAsdu(asdu) => Event::SendAsdu(asdu),
        Command::StartDt => Event::StartDt,
        Command::StopDt => Event::StopDt,
    }
}

/// The session clock: tokio's, so that a paused clock drives the timers.
fn now() -> Instant {
    TokioInstant::now().into_std()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn a_connection_not_established_within_t0_times_out() {
        let t0 = Duration::from_secs(30);
        let started = TokioInstant::now();
        let result = within_t0(t0, std::future::pending::<io::Result<()>>()).await;
        assert!(matches!(result, Err(TransportError::ConnectTimeout)));
        assert_eq!(started.elapsed(), t0);
    }
}
