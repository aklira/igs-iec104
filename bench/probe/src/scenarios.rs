// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! The scenarios of task L4. `d1-*` scenarios are run by igs-iec104 as the
//! controlling station, against the reference server; `d2-*` scenarios are run
//! as the controlled station, for the reference client to connect to. The
//! wire checks are made from the capture, in the bench tests.

use std::net::SocketAddr;
use std::time::Duration;

use igs_iec104::{connect, Command, Delivery};
use igs_iec104_codec::header::cause;
use igs_iec104_link::{LinkConfig, Parameters, Rejection, Role, TransferState};
use tokio::net::TcpListener;
use tokio::net::TcpStream;
use tokio::time::{sleep, timeout};

use crate::link::{
    interrogation, interrogation_cause, interrogation_with_cause, reply_to, single_point,
    transfer_is, Failure, Link,
};

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
