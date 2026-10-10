// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! Tests of the driver. The peer is either a raw end of an in-memory stream,
//! which the test drives frame by frame, or a TCP loopback connection. The
//! in-memory tests run on a paused clock, so every delay is exact.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use std::time::Duration;

use igs_iec104::{connect, run, Command, Delivery, TransportError};
use igs_iec104_codec::apci::{Apdu, SequenceNumber};
use igs_iec104_codec::asdu::Asdu;
use igs_iec104_link::{CloseReason, LinkConfig, Rejection, Role, TransferState};
use tokio::io::{duplex, AsyncReadExt, AsyncWriteExt, DuplexStream};
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio::time::Instant;

/// C_IC_NA_1, activation, station interrogation: the ASDU of the tests.
const INTERROGATION: &[u8] = &[0x64, 0x01, 0x06, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x14];

/// The U frames of the tests, as they are on the wire (§5.2, §5.3).
const STARTDT_ACT: [u8; 6] = [0x68, 0x04, 0x07, 0x00, 0x00, 0x00];
const STARTDT_CON: [u8; 6] = [0x68, 0x04, 0x0B, 0x00, 0x00, 0x00];
const TESTFR_ACT: [u8; 6] = [0x68, 0x04, 0x43, 0x00, 0x00, 0x00];
const TESTFR_CON: [u8; 6] = [0x68, 0x04, 0x83, 0x00, 0x00, 0x00];

fn asdu() -> Asdu {
    Asdu::decode(INTERROGATION).expect("sample decodes")
}

fn controlling() -> LinkConfig {
    LinkConfig::with_defaults(Role::Controlling)
}

fn controlled() -> LinkConfig {
    LinkConfig::with_defaults(Role::Controlled)
}

/// One connection under test: the raw peer end of its stream, and its channels.
struct Harness {
    peer: DuplexStream,
    commands: mpsc::Sender<Command>,
    deliveries: mpsc::Receiver<Delivery>,
    task: JoinHandle<Result<(), TransportError>>,
}

fn start(config: LinkConfig) -> Harness {
    let (local, peer) = duplex(4096);
    let (commands, commands_rx) = mpsc::channel(8);
    let (deliveries_tx, deliveries) = mpsc::channel(8);
    let task = tokio::spawn(run(local, config, commands_rx, deliveries_tx));
    Harness {
        peer,
        commands,
        deliveries,
        task,
    }
}

async fn read_frame(peer: &mut DuplexStream) -> [u8; 6] {
    let mut frame = [0u8; 6];
    peer.read_exact(&mut frame).await.expect("a frame arrives");
    frame
}

#[tokio::test(start_paused = true)]
async fn a_start_act_without_confirmation_closes_the_connection_after_t1() {
    let mut link = start(controlling());
    let started = Instant::now();
    link.commands
        .send(Command::StartDt)
        .await
        .expect("the driver runs");
    assert_eq!(read_frame(&mut link.peer).await, STARTDT_ACT);
    let result = link.task.await.expect("the task does not panic");
    assert!(matches!(
        result,
        Err(TransportError::Closed(CloseReason::T1Expired))
    ));
    assert_eq!(started.elapsed(), Duration::from_secs(15));
}

#[tokio::test(start_paused = true)]
async fn an_idle_connection_is_tested_after_t3_and_closed_after_t1() {
    let mut link = start(controlled());
    let started = Instant::now();
    assert_eq!(read_frame(&mut link.peer).await, TESTFR_ACT);
    assert_eq!(started.elapsed(), Duration::from_secs(20));
    let result = link.task.await.expect("the task does not panic");
    assert!(matches!(
        result,
        Err(TransportError::Closed(CloseReason::T1Expired))
    ));
    assert_eq!(started.elapsed(), Duration::from_secs(35));
}

#[tokio::test(start_paused = true)]
async fn a_test_confirmation_keeps_the_connection_open_and_restarts_t3() {
    let mut link = start(controlled());
    let started = Instant::now();
    assert_eq!(read_frame(&mut link.peer).await, TESTFR_ACT);
    link.peer
        .write_all(&TESTFR_CON)
        .await
        .expect("the peer writes");
    // The confirmation restarted t3: the next test is due 20 s later.
    assert_eq!(read_frame(&mut link.peer).await, TESTFR_ACT);
    assert_eq!(started.elapsed(), Duration::from_secs(40));
    link.peer
        .write_all(&TESTFR_CON)
        .await
        .expect("the peer writes");
    drop(link.commands);
    assert!(link.task.await.expect("the task does not panic").is_ok());
}

#[tokio::test(start_paused = true)]
async fn dropping_the_commands_shuts_the_stream_down() {
    let mut link = start(controlling());
    drop(link.commands);
    assert!(link.task.await.expect("the task does not panic").is_ok());
    let mut byte = [0u8; 1];
    let read = link
        .peer
        .read(&mut byte)
        .await
        .expect("the read ends cleanly");
    assert_eq!(read, 0, "the peer reads the end of the stream");
}

#[tokio::test(start_paused = true)]
async fn the_peer_closing_the_stream_is_reported() {
    let link = start(controlling());
    drop(link.peer);
    let result = link.task.await.expect("the task does not panic");
    assert!(matches!(result, Err(TransportError::PeerClosed)));
}

#[tokio::test(start_paused = true)]
async fn a_frame_that_cannot_be_decoded_closes_the_connection() {
    let mut link = start(controlled());
    link.peer
        .write_all(&[0x00, 0x04, 0x07, 0x00, 0x00, 0x00])
        .await
        .expect("the peer writes");
    let result = link.task.await.expect("the task does not panic");
    assert!(matches!(
        result,
        Err(TransportError::Closed(CloseReason::Decode(_)))
    ));
}

#[tokio::test(start_paused = true)]
async fn the_transfer_state_is_reported_to_the_application() {
    let mut link = start(controlling());
    link.commands
        .send(Command::StartDt)
        .await
        .expect("the driver runs");
    assert_eq!(read_frame(&mut link.peer).await, STARTDT_ACT);
    assert_eq!(
        link.deliveries.recv().await,
        Some(Delivery::Transfer(TransferState::PendingStarted))
    );
    link.peer
        .write_all(&STARTDT_CON)
        .await
        .expect("the peer writes");
    assert_eq!(
        link.deliveries.recv().await,
        Some(Delivery::Transfer(TransferState::Started))
    );
    drop(link.commands);
    assert!(link.task.await.expect("the task does not panic").is_ok());
}

#[tokio::test(start_paused = true)]
async fn an_i_frame_from_the_peer_is_delivered_to_the_application() {
    let mut link = start(controlled());
    link.peer
        .write_all(&STARTDT_ACT)
        .await
        .expect("the peer writes");
    assert_eq!(read_frame(&mut link.peer).await, STARTDT_CON);
    assert_eq!(
        link.deliveries.recv().await,
        Some(Delivery::Transfer(TransferState::Started))
    );
    let frame = Apdu::Information {
        send: SequenceNumber::ZERO,
        receive: SequenceNumber::ZERO,
        asdu: asdu(),
    }
    .to_vec()
    .expect("the sample encodes");
    link.peer.write_all(&frame).await.expect("the peer writes");
    assert_eq!(link.deliveries.recv().await, Some(Delivery::Asdu(asdu())));
    drop(link.commands);
    assert!(link.task.await.expect("the task does not panic").is_ok());
}

#[tokio::test(start_paused = true)]
async fn a_command_while_the_transfer_is_stopped_is_returned() {
    let mut link = start(controlling());
    link.commands
        .send(Command::SendAsdu(asdu()))
        .await
        .expect("the driver runs");
    assert_eq!(
        link.deliveries.recv().await,
        Some(Delivery::Rejected(Rejection::TransferStopped(asdu())))
    );
    drop(link.commands);
    assert!(link.task.await.expect("the task does not panic").is_ok());
}

/// D-014: an ASDU that the peer has not acknowledged when the connection ends is handed back to the
/// application once (clauses 10.5 and 10.6). The driver does not send it again on its own.
#[tokio::test(start_paused = true)]
async fn an_asdu_the_peer_does_not_acknowledge_is_returned_when_the_connection_ends() {
    let mut link = start(controlling());
    link.commands
        .send(Command::StartDt)
        .await
        .expect("the driver runs");
    assert_eq!(read_frame(&mut link.peer).await, STARTDT_ACT);
    assert_eq!(
        link.deliveries.recv().await,
        Some(Delivery::Transfer(TransferState::PendingStarted))
    );
    link.peer
        .write_all(&STARTDT_CON)
        .await
        .expect("the peer writes");
    assert_eq!(
        link.deliveries.recv().await,
        Some(Delivery::Transfer(TransferState::Started))
    );

    // The I frame carries the ASDU with N(S) = 0 and N(R) = 0 (§5.1). The peer reads it and then
    // closes the stream without acknowledging it.
    link.commands
        .send(Command::SendAsdu(asdu()))
        .await
        .expect("the driver runs");
    let mut frame = [0u8; 16];
    link.peer
        .read_exact(&mut frame)
        .await
        .expect("the I frame arrives");
    assert_eq!(frame[..6], [0x68, 14, 0, 0, 0, 0]);
    drop(link.peer);

    assert_eq!(
        link.deliveries.recv().await,
        Some(Delivery::Unacknowledged(vec![asdu()]))
    );
    let result = link.task.await.expect("the task does not panic");
    assert!(matches!(result, Err(TransportError::PeerClosed)));
}

#[tokio::test]
async fn a_tcp_loopback_carries_a_start_and_an_asdu() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("binds");
    let address = listener.local_addr().expect("has an address");
    let accept = tokio::spawn(async move { listener.accept().await });

    let client_stream = connect(address, Duration::from_secs(30))
        .await
        .expect("connects");
    let (server_stream, _) = accept
        .await
        .expect("the accept task runs")
        .expect("accepts");

    let (client_commands, client_commands_rx) = mpsc::channel(8);
    let (client_deliveries_tx, mut client_deliveries) = mpsc::channel(8);
    let (server_commands, server_commands_rx) = mpsc::channel(8);
    let (server_deliveries_tx, mut server_deliveries) = mpsc::channel(8);
    let client = tokio::spawn(run(
        client_stream,
        controlling(),
        client_commands_rx,
        client_deliveries_tx,
    ));
    let server = tokio::spawn(run(
        server_stream,
        controlled(),
        server_commands_rx,
        server_deliveries_tx,
    ));

    client_commands
        .send(Command::StartDt)
        .await
        .expect("the client runs");
    assert_eq!(
        client_deliveries.recv().await,
        Some(Delivery::Transfer(TransferState::PendingStarted))
    );
    assert_eq!(
        client_deliveries.recv().await,
        Some(Delivery::Transfer(TransferState::Started))
    );
    client_commands
        .send(Command::SendAsdu(asdu()))
        .await
        .expect("the client runs");
    assert_eq!(
        server_deliveries.recv().await,
        Some(Delivery::Transfer(TransferState::Started))
    );
    assert_eq!(server_deliveries.recv().await, Some(Delivery::Asdu(asdu())));

    drop(client_commands);
    assert!(client
        .await
        .expect("the client task does not panic")
        .is_ok());
    assert!(matches!(
        server.await.expect("the server task does not panic"),
        Err(TransportError::PeerClosed)
    ));
    drop(server_commands);
}

#[tokio::test]
async fn connecting_to_a_closed_port_reports_the_error() {
    let address = {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("binds");
        listener.local_addr().expect("has an address")
    };
    let result = connect(address, Duration::from_secs(30)).await;
    assert!(matches!(result, Err(TransportError::Connect(_))));
}
