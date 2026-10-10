// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! The scenarios of task L4. `d1-*` scenarios are run by igs-iec104 as the
//! controlling station, against the reference server; `d2-*` scenarios are run
//! as the controlled station, for the reference client to connect to. The
//! wire checks are made from the capture, in the bench tests.

use std::net::SocketAddr;
use std::time::{Duration, Instant};

use igs_iec104::{connect, Command, Delivery, TransportError};
use igs_iec104_codec::apci::SequenceNumber;
use igs_iec104_codec::header::cause;
use igs_iec104_link::{CloseReason, LinkConfig, Parameters, Rejection, Role, TransferState};
use tokio::net::TcpListener;
use tokio::net::TcpStream;
use tokio::time::{sleep, timeout};

use crate::link::{
    interrogation, interrogation_cause, interrogation_with_cause, reply_to, single_point,
    transfer_is, Failure, Link,
};
use crate::relay::{bridge, Fault, Kind, Rule, Toward};

/// How long a reply may take.
const REPLY: Duration = Duration::from_secs(30);
/// How long to wait for the peer to connect or to start.
const ACCEPT: Duration = Duration::from_secs(120);
/// Idle time of the controlling probe: longer than t3 (20 s) and than the
/// probe's t1 (15 s) after the test act, so a missing confirmation closes the
/// connection before the end.
const IDLE_CONTROLLING: Duration = Duration::from_secs(40);
/// Idle time of the controlled probe: longer than the reference client's t3.
const IDLE_CONTROLLED: Duration = Duration::from_secs(50);
/// Idle time when the peer runs the test: its t3 (20 s) must expire within it.
const IDLE_PEER_TEST: Duration = Duration::from_secs(40);
/// Frames the probe must get accepted in the wrap test: more than 32768, so N(S)
/// goes through 32767 and back to 0.
const WRAP_ACCEPTED: usize = 33_000;
/// Pause between two frames of the wrap test: the acknowledgements come back.
const WRAP_PAUSE: Duration = Duration::from_micros(500);
/// Interrogations sent at once: more than k = 12, so the window fills.
const WINDOW_BURST: usize = 40;

/// Runs the scenario called `name` against the peer at `address`.
pub async fn run(name: &str, address: SocketAddr) -> Result<(), Failure> {
    match name {
        "d1-startdt-gi-stopdt" => controlling_start_interrogate_stop(address).await,
        "d1-testfr-idle" => controlling_idle(address).await,
        "d1-window" => controlling_window(address).await,
        "d2-startdt-gi" => controlled_start_interrogate(address).await,
        "d2-testfr-idle" => controlled_idle(address).await,
        "d2-ack-after-w" => controlled_acknowledge(address).await,
        "d1-testfr-from-peer" => controlling_peer_test(address).await,
        "d2-testfr-from-peer" => controlled_peer_test(address).await,
        "d2-wrap" => controlled_wrap(address).await,
        "d1-fault-t1-silent" => controlling_t1_silent(address).await,
        "d1-fault-ack-within-t1" => controlling_ack_within_t1(address).await,
        "d1-fault-ack-beyond-t1" => controlling_ack_beyond_t1(address).await,
        "d1-fault-dropped" => controlling_dropped(address).await,
        "d1-fault-duplicated" => controlling_duplicated(address).await,
        "d2-fault-t1-silent" => controlled_t1_silent(address).await,
        "d2-fault-ack-late" => controlled_ack_late(address).await,
        "d2-fault-dropped" => controlled_dropped(address).await,
        "d2-fault-duplicated" => controlled_duplicated(address).await,
        other => Err(format!("unknown scenario {other}").into()),
    }
}

async fn controlling_link(address: SocketAddr) -> Result<Link, Failure> {
    let config = LinkConfig::with_defaults(Role::Controlling);
    let stream = connect(address, config.parameters().t0).await?;
    Ok(Link::spawn(stream, config))
}

/// Accepts one connection on `address`, for the reference client to connect to.
async fn accept_one(address: SocketAddr) -> Result<TcpStream, Failure> {
    let listener = TcpListener::bind(address)
        .await
        .map_err(|error| format!("cannot listen on {address}: {error}"))?;
    println!("listening on {address}");
    let (stream, peer) = timeout(ACCEPT, listener.accept())
        .await
        .map_err(|_| "no connection was accepted")??;
    println!("accepted {peer}");
    Ok(stream)
}

/// d1: start, general interrogation, stop; the probe is the controlling station.
async fn controlling_start_interrogate_stop(address: SocketAddr) -> Result<(), Failure> {
    let mut link = controlling_link(address).await?;
    link.command(Command::StartDt).await?;
    link.expect("STARTDT con", REPLY, transfer_is(TransferState::Started))
        .await?;
    link.command(Command::SendAsdu(interrogation(cause::ACTIVATION)?))
        .await?;
    link.expect(
        "ACTCON of the interrogation",
        REPLY,
        interrogation_with_cause(cause::ACTIVATION_CONFIRMATION),
    )
    .await?;
    link.expect(
        "ACTTERM of the interrogation",
        REPLY,
        interrogation_with_cause(cause::ACTIVATION_TERMINATION),
    )
    .await?;
    link.command(Command::StopDt).await?;
    link.expect("STOPDT con", REPLY, transfer_is(TransferState::Stopped))
        .await?;
    link.finish().await
}

/// d1: idle after the start; the probe's test act is confirmed and the
/// connection stays open.
async fn controlling_idle(address: SocketAddr) -> Result<(), Failure> {
    let mut link = controlling_link(address).await?;
    link.command(Command::StartDt).await?;
    link.expect("STARTDT con", REPLY, transfer_is(TransferState::Started))
        .await?;
    sleep(IDLE_CONTROLLING).await;
    if link.ended() {
        return Err("the connection closed while idle: the test act was not confirmed".into());
    }
    link.finish().await
}

/// d1: more interrogations than k at once. Each one is answered, and the
/// window refuses the ones that do not fit.
async fn controlling_window(address: SocketAddr) -> Result<(), Failure> {
    let mut link = controlling_link(address).await?;
    link.command(Command::StartDt).await?;
    link.expect("STARTDT con", REPLY, transfer_is(TransferState::Started))
        .await?;
    for _ in 0..WINDOW_BURST {
        link.command(Command::SendAsdu(interrogation(cause::ACTIVATION)?))
            .await?;
    }
    let (mut confirmed, mut terminated, mut rejected) = (0usize, 0usize, 0usize);
    timeout(ACCEPT, async {
        while terminated.saturating_add(rejected) < WINDOW_BURST {
            match link.next().await {
                Some(Delivery::Rejected(Rejection::WindowFull(_))) => {
                    rejected = rejected.saturating_add(1);
                }
                Some(Delivery::Rejected(other)) => {
                    return Err(format!("unexpected rejection {other:?}"))
                }
                Some(delivery) => match interrogation_cause(&delivery) {
                    Some(cause::ACTIVATION_CONFIRMATION) => {
                        confirmed = confirmed.saturating_add(1);
                    }
                    Some(cause::ACTIVATION_TERMINATION) => {
                        terminated = terminated.saturating_add(1);
                    }
                    _ => {}
                },
                None => {
                    return Err(
                        "the connection ended before every interrogation was answered".into(),
                    )
                }
            }
        }
        Ok(())
    })
    .await
    .map_err(|_| "the interrogations were not answered in time")??;
    println!("window: {terminated} answered, {rejected} refused as window full");
    if rejected == 0 {
        return Err("the window never filled: no ASDU was refused".into());
    }
    if confirmed != terminated {
        return Err(format!("{confirmed} confirmations for {terminated} terminations").into());
    }
    link.finish().await
}

/// d2: the reference client starts the transfer and interrogates; the probe
/// confirms and terminates the interrogation.
async fn controlled_start_interrogate(address: SocketAddr) -> Result<(), Failure> {
    let stream = accept_one(address).await?;
    let mut link = Link::spawn(stream, LinkConfig::with_defaults(Role::Controlled));
    link.expect(
        "STARTDT from the peer",
        ACCEPT,
        transfer_is(TransferState::Started),
    )
    .await?;
    let Delivery::Asdu(request) = link
        .expect(
            "interrogation from the peer",
            ACCEPT,
            interrogation_with_cause(cause::ACTIVATION),
        )
        .await?
    else {
        return Err("the interrogation is not an ASDU".into());
    };
    link.command(Command::SendAsdu(reply_to(
        &request,
        cause::ACTIVATION_CONFIRMATION,
    )?))
    .await?;
    link.command(Command::SendAsdu(reply_to(
        &request,
        cause::ACTIVATION_TERMINATION,
    )?))
    .await?;
    // Let the peer read the termination before the connection is closed.
    sleep(Duration::from_secs(3)).await;
    link.finish().await
}

/// d2: idle after the start. The probe's own test act (t3) is confirmed by the
/// reference client, and the connection stays open.
async fn controlled_idle(address: SocketAddr) -> Result<(), Failure> {
    let stream = accept_one(address).await?;
    let mut link = Link::spawn(stream, LinkConfig::with_defaults(Role::Controlled));
    link.expect(
        "STARTDT from the peer",
        ACCEPT,
        transfer_is(TransferState::Started),
    )
    .await?;
    sleep(IDLE_CONTROLLED).await;
    if link.ended() {
        return Err("the connection closed while the peer tested it".into());
    }
    link.finish().await
}

/// d2: the probe sends eight, then three more, spontaneous single points. The
/// peer must acknowledge the eight at once (w = 8) and the three within t2.
async fn controlled_acknowledge(address: SocketAddr) -> Result<(), Failure> {
    let stream = accept_one(address).await?;
    let mut link = Link::spawn(stream, LinkConfig::with_defaults(Role::Controlled));
    link.expect(
        "STARTDT from the peer",
        ACCEPT,
        transfer_is(TransferState::Started),
    )
    .await?;
    for address in 672u32..680 {
        link.command(Command::SendAsdu(single_point(address, true)?))
            .await?;
    }
    sleep(Duration::from_secs(3)).await;
    for address in 680u32..683 {
        link.command(Command::SendAsdu(single_point(address, false)?))
            .await?;
    }
    // Our t1 (15 s) runs from the last frame sent: the acknowledgement must come first.
    sleep(Duration::from_secs(12)).await;
    if link.ended() {
        return Err("the connection closed: the three frames were not acknowledged in time".into());
    }
    link.finish().await
}

/// A t3 longer than any test: the probe then sends no test act, so that any test
/// act on the wire comes from the peer.
fn long_t3() -> Result<LinkConfig, Failure> {
    let parameters = Parameters {
        t3: Duration::from_secs(300),
        ..Parameters::default()
    };
    Ok(LinkConfig::new(Role::Controlling, parameters)?)
}

/// d1: the probe is silent, so the reference server runs the test (its t3); the
/// probe confirms it and the connection stays open.
async fn controlling_peer_test(address: SocketAddr) -> Result<(), Failure> {
    let config = long_t3()?;
    let stream = connect(address, config.parameters().t0).await?;
    let mut link = Link::spawn(stream, config);
    link.command(Command::StartDt).await?;
    link.expect("STARTDT con", REPLY, transfer_is(TransferState::Started))
        .await?;
    sleep(IDLE_PEER_TEST).await;
    if link.ended() {
        return Err("the connection closed: the peer's test act was not confirmed".into());
    }
    link.finish().await
}

/// d2: the probe is silent after the start, so the reference client runs the
/// test (its t3); the probe confirms it and the connection stays open.
async fn controlled_peer_test(address: SocketAddr) -> Result<(), Failure> {
    let stream = accept_one(address).await?;
    let parameters = Parameters {
        t3: Duration::from_secs(300),
        ..Parameters::default()
    };
    let config = LinkConfig::new(Role::Controlled, parameters)?;
    let mut link = Link::spawn(stream, config);
    link.expect(
        "STARTDT from the peer",
        ACCEPT,
        transfer_is(TransferState::Started),
    )
    .await?;
    sleep(IDLE_PEER_TEST).await;
    if link.ended() {
        return Err("the connection closed: the peer's test act was not confirmed".into());
    }
    link.finish().await
}

/// d2: more than 32768 I frames from the probe, so that N(S) wraps. Frames the
/// window refuses are not counted; the connection must stay open throughout.
async fn controlled_wrap(address: SocketAddr) -> Result<(), Failure> {
    let stream = accept_one(address).await?;
    let mut link = Link::spawn(stream, LinkConfig::with_defaults(Role::Controlled));
    link.expect(
        "STARTDT from the peer",
        ACCEPT,
        transfer_is(TransferState::Started),
    )
    .await?;
    let (mut attempts, mut refused) = (0usize, 0usize);
    while attempts.saturating_sub(refused) < WRAP_ACCEPTED {
        // Eight addresses in turn: the value does not matter, only the count does.
        let address = 672u32.saturating_add(u32::try_from(attempts % 8).unwrap_or(0));
        link.command(Command::SendAsdu(single_point(address, true)?))
            .await?;
        attempts = attempts.saturating_add(1);
        sleep(WRAP_PAUSE).await;
        while let Some(delivery) = link.try_next() {
            match delivery {
                Delivery::Rejected(Rejection::WindowFull(_)) => {
                    refused = refused.saturating_add(1);
                }
                Delivery::Rejected(other) => {
                    return Err(format!("unexpected rejection {other:?}").into())
                }
                _ => {}
            }
        }
        if link.ended() {
            // The reason is the driver's error, if it has one.
            link.finish().await?;
            return Err("the connection closed while the frames were sent".into());
        }
    }
    println!(
        "wrap: {} frames accepted, {refused} refused",
        attempts.saturating_sub(refused)
    );
    sleep(Duration::from_secs(3)).await;
    if link.ended() {
        return Err("the connection closed after the frames".into());
    }
    link.finish().await
}

/// A rule over the frames of `kind` going `toward`, from index `first`, `count` of them.
fn rule(toward: Toward, kind: Kind, first: usize, count: Option<usize>, fault: Fault) -> Rule {
    Rule {
        toward,
        kind,
        first,
        count,
        fault,
    }
}

/// The driver connects to the peer through a relay that applies `rules`.
async fn controlling_through_relay(peer: SocketAddr, rules: Vec<Rule>) -> Result<Link, Failure> {
    let relay = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|error| format!("cannot listen for the relay: {error}"))?;
    let relay_address = relay.local_addr()?;
    tokio::spawn(async move {
        let (driver, _) = relay.accept().await?;
        let upstream = TcpStream::connect(peer).await?;
        bridge(driver, upstream, &rules).await
    });
    let config = LinkConfig::with_defaults(Role::Controlling);
    let stream = connect(relay_address, config.parameters().t0).await?;
    Ok(Link::spawn(stream, config))
}

/// The reference client connects to `address` through a relay that applies
/// `rules`; the relay hands the connection to the controlled driver.
async fn controlled_through_relay(address: SocketAddr, rules: Vec<Rule>) -> Result<Link, Failure> {
    let driver_listener = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|error| format!("cannot listen for the driver: {error}"))?;
    let driver_address = driver_listener.local_addr()?;
    let peer = accept_one(address).await?;
    tokio::spawn(async move {
        let driver = TcpStream::connect(driver_address).await?;
        bridge(driver, peer, &rules).await
    });
    let (stream, _) = timeout(ACCEPT, driver_listener.accept())
        .await
        .map_err(|_| "the relay did not connect to the driver")??;
    Ok(Link::spawn(
        stream,
        LinkConfig::with_defaults(Role::Controlled),
    ))
}

/// The connection ended with the t1 expiry.
fn expect_t1_expiry(error: &TransportError) -> Result<(), Failure> {
    match error {
        TransportError::Closed(CloseReason::T1Expired) => {
            println!("closed by t1");
            Ok(())
        }
        other => Err(format!("expected the t1 expiry, got: {other}").into()),
    }
}

/// The peer closed the connection (the stream ended or failed).
fn expect_peer_closed(error: &TransportError) -> Result<(), Failure> {
    match error {
        TransportError::PeerClosed | TransportError::Io(_) => {
            println!("closed by the peer: {error}");
            Ok(())
        }
        other => Err(format!("expected the peer to close, got: {other}").into()),
    }
}

/// The time since `since` is `expected` seconds, within 2 s.
fn check_elapsed(since: Instant, expected: f64) -> Result<(), Failure> {
    let elapsed = since.elapsed().as_secs_f64();
    println!("{elapsed:.1} s after the event");
    if (elapsed - expected).abs() <= 2.0 {
        Ok(())
    } else {
        Err(format!("after {elapsed:.1} s, expected about {expected} s").into())
    }
}

/// d1: the relay drops every frame from the peer. The start act is never
/// confirmed, and t1 closes the connection 15 s after it.
async fn controlling_t1_silent(address: SocketAddr) -> Result<(), Failure> {
    let rules = vec![rule(Toward::Driver, Kind::Any, 0, None, Fault::Drop)];
    let link = controlling_through_relay(address, rules).await?;
    let sent = Instant::now();
    link.command(Command::StartDt).await?;
    let error = link.closed_by_itself(Duration::from_secs(30)).await?;
    expect_t1_expiry(&error)?;
    check_elapsed(sent, 15.0)
}

/// d1: the relay delays the peer's I frames by 3 s. The acknowledgement of our
/// interrogation comes within t1, so the connection stays open.
async fn controlling_ack_within_t1(address: SocketAddr) -> Result<(), Failure> {
    let delay = Fault::Delay(Duration::from_secs(3));
    let rules = vec![rule(Toward::Driver, Kind::Information, 0, None, delay)];
    let mut link = controlling_through_relay(address, rules).await?;
    link.command(Command::StartDt).await?;
    link.expect("STARTDT con", REPLY, transfer_is(TransferState::Started))
        .await?;
    link.command(Command::SendAsdu(interrogation(cause::ACTIVATION)?))
        .await?;
    link.expect(
        "ACTTERM of the interrogation",
        REPLY,
        interrogation_with_cause(cause::ACTIVATION_TERMINATION),
    )
    .await?;
    sleep(Duration::from_secs(2)).await;
    if link.ended() {
        return Err("the connection closed although the acknowledgement came within t1".into());
    }
    link.finish().await
}

/// d1: the relay delays the peer's I frames by 20 s. The interrogation is not
/// acknowledged within t1 (15 s), and the connection closes.
async fn controlling_ack_beyond_t1(address: SocketAddr) -> Result<(), Failure> {
    let delay = Fault::Delay(Duration::from_secs(20));
    let rules = vec![rule(Toward::Driver, Kind::Information, 0, None, delay)];
    let mut link = controlling_through_relay(address, rules).await?;
    link.command(Command::StartDt).await?;
    link.expect("STARTDT con", REPLY, transfer_is(TransferState::Started))
        .await?;
    let sent = Instant::now();
    link.command(Command::SendAsdu(interrogation(cause::ACTIVATION)?))
        .await?;
    let error = link.closed_by_itself(Duration::from_secs(30)).await?;
    expect_t1_expiry(&error)?;
    check_elapsed(sent, 15.0)
}

/// d1: the relay drops our second interrogation. The peer receives the third one
/// out of sequence and closes the connection (figure 11).
async fn controlling_dropped(address: SocketAddr) -> Result<(), Failure> {
    let rules = vec![rule(
        Toward::Peer,
        Kind::Information,
        1,
        Some(1),
        Fault::Drop,
    )];
    let mut link = controlling_through_relay(address, rules).await?;
    link.command(Command::StartDt).await?;
    link.expect("STARTDT con", REPLY, transfer_is(TransferState::Started))
        .await?;
    for _ in 0..3 {
        link.command(Command::SendAsdu(interrogation(cause::ACTIVATION)?))
            .await?;
    }
    let error = link.closed_by_itself(Duration::from_secs(30)).await?;
    expect_peer_closed(&error)
}

/// d1: the relay duplicates our first interrogation. The peer receives the same
/// N(S) twice and closes the connection (figure 11).
async fn controlling_duplicated(address: SocketAddr) -> Result<(), Failure> {
    let rules = vec![rule(
        Toward::Peer,
        Kind::Information,
        0,
        Some(1),
        Fault::Duplicate,
    )];
    let mut link = controlling_through_relay(address, rules).await?;
    link.command(Command::StartDt).await?;
    link.expect("STARTDT con", REPLY, transfer_is(TransferState::Started))
        .await?;
    link.command(Command::SendAsdu(interrogation(cause::ACTIVATION)?))
        .await?;
    let error = link.closed_by_itself(Duration::from_secs(30)).await?;
    expect_peer_closed(&error)
}

/// d2: the relay drops the acknowledgements of the reference client. Our
/// spontaneous values are never acknowledged, and t1 closes the connection 15 s
/// after the last one.
async fn controlled_t1_silent(address: SocketAddr) -> Result<(), Failure> {
    let rules = vec![rule(
        Toward::Driver,
        Kind::Supervisory,
        0,
        None,
        Fault::Drop,
    )];
    let mut link = controlled_through_relay(address, rules).await?;
    link.expect(
        "STARTDT from the peer",
        ACCEPT,
        transfer_is(TransferState::Started),
    )
    .await?;
    // The peer's own interrogation goes out at once with N(R) = 0 if our frames come after
    // it: then the relay sees no acknowledgement of ours in it.
    sleep(Duration::from_secs(1)).await;
    for address in 672u32..675 {
        link.command(Command::SendAsdu(single_point(address, true)?))
            .await?;
    }
    let sent = Instant::now();
    let error = link.closed_by_itself(Duration::from_secs(30)).await?;
    expect_t1_expiry(&error)?;
    check_elapsed(sent, 15.0)
}

/// d2: the relay holds our acknowledgements for 20 s. The reference client's t1
/// for its interrogation expires first, and it closes the connection (figure 12).
async fn controlled_ack_late(address: SocketAddr) -> Result<(), Failure> {
    let delay = Fault::Delay(Duration::from_secs(20));
    let rules = vec![rule(Toward::Peer, Kind::Supervisory, 0, None, delay)];
    let mut link = controlled_through_relay(address, rules).await?;
    link.expect(
        "STARTDT from the peer",
        ACCEPT,
        transfer_is(TransferState::Started),
    )
    .await?;
    link.expect(
        "interrogation from the peer",
        ACCEPT,
        interrogation_with_cause(cause::ACTIVATION),
    )
    .await?;
    let received = Instant::now();
    let error = link.closed_by_itself(Duration::from_secs(40)).await?;
    expect_peer_closed(&error)?;
    check_elapsed(received, 15.0)
}

/// d2: the relay drops our second spontaneous value. The reference client
/// receives the third one out of sequence and closes the connection (figure 11).
async fn controlled_dropped(address: SocketAddr) -> Result<(), Failure> {
    let rules = vec![rule(
        Toward::Peer,
        Kind::Information,
        1,
        Some(1),
        Fault::Drop,
    )];
    let mut link = controlled_through_relay(address, rules).await?;
    link.expect(
        "STARTDT from the peer",
        ACCEPT,
        transfer_is(TransferState::Started),
    )
    .await?;
    for address in 672u32..675 {
        link.command(Command::SendAsdu(single_point(address, true)?))
            .await?;
    }
    let error = link.closed_by_itself(Duration::from_secs(30)).await?;
    expect_peer_closed(&error)
}

/// d2: the relay duplicates the first frame of the reference client. Our session
/// takes the copy for a frame out of sequence and closes with a sequence error.
async fn controlled_duplicated(address: SocketAddr) -> Result<(), Failure> {
    let rules = vec![rule(
        Toward::Driver,
        Kind::Information,
        0,
        Some(1),
        Fault::Duplicate,
    )];
    let link = controlled_through_relay(address, rules).await?;
    let error = link.closed_by_itself(Duration::from_secs(30)).await?;
    match error {
        TransportError::Closed(CloseReason::SequenceError { expected, received })
            if expected == SequenceNumber::new(1).ok_or("1 is in range")?
                && received == SequenceNumber::new(0).ok_or("0 is in range")? =>
        {
            println!(
                "closed on a sequence error: expected {}, received {}",
                expected.value(),
                received.value()
            );
            Ok(())
        }
        other => Err(format!("expected a sequence error, got: {other}").into()),
    }
}
