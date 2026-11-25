// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! The load campaign: general interrogations of every point of the station, on one connection
//! that also carries spontaneous changes.
//!
//! The station has `--points` float points, all in group 1. The first `--changing` of them change
//! in turn, `--rate` changes a second in all. Each change carries a sequence number as its value,
//! and the campaign keeps the instant of every change, so that the time from a change to its
//! reception is measured on the same clock. A general interrogation starts `--every-ms` after the
//! previous one ended, and its time runs from the request to the termination.

use std::collections::BTreeSet;
use std::error::Error;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use igs_iec104::client::{Client, ClientConfig, Event};
use igs_iec104::process_image::PointValue;
use igs_iec104::server::ServerHandle;
use igs_iec104::Delivery;
use igs_iec104_codec::asdu::{Asdu, Body, Objects};
use igs_iec104_codec::elements::{Qoi, ShortFloat};
use igs_iec104_codec::header::{cause, CommonAddress, InformationObjectAddress};
use igs_iec104_link::TransferState;
use tokio::time::{interval, sleep_until, Instant as TokioInstant};

use crate::{option, quality, start, stats, time, STATION};

/// The changes are made every 10 ms: `rate / 100` of them each time.
const TICK: Duration = Duration::from_millis(10);

/// Runs the campaign with the options in `args`. Returns whether it passed.
pub async fn run(args: &[String]) -> Result<bool, Box<dyn Error>> {
    let seconds: u64 = option(args, "--seconds", 60)?;
    let points: u32 = option(args, "--points", 10_000)?;
    let changing: u32 = option(args, "--changing", 1_000)?;
    let rate: u32 = option(args, "--rate", 1_000)?;
    let every_ms: u64 = option(args, "--every-ms", 250)?;
    if rate < 100 || changing == 0 || changing > points {
        return Err("the rate is at least 100, and 1 to --points points change".into());
    }

    let (address, handle) = start(STATION, &[], points).await?;
    let sent = Arc::new(Mutex::new(Vec::<Instant>::new()));
    let end = Instant::now()
        .checked_add(Duration::from_secs(seconds))
        .ok_or("the duration")?;
    let driver = tokio::spawn(drive(handle, changing, rate, Arc::clone(&sent), end));

    let mut client = Client::connect(ClientConfig::new(address, CommonAddress::new(STATION)));
    start_transfer(&mut client).await?;
    let outcome = measure(
        &mut client,
        &sent,
        end,
        Duration::from_millis(every_ms),
        points,
    )
    .await?;
    let sent_changes = driver.await??;

    let expected = usize::try_from(points)?;
    for missing in &outcome.gi_missing {
        println!("missing from an interrogation: {missing:?}");
    }
    let interrogations_ok = outcome.gi_counts.iter().all(|count| *count == expected);
    let (min, max) = (
        outcome.gi_counts.iter().min().copied().unwrap_or(0),
        outcome.gi_counts.iter().max().copied().unwrap_or(0),
    );
    println!("# Load campaign");
    println!();
    println!(
        "One connection on loopback, {seconds} s. The station has {points} float points in group 1; \
         the first {changing} change in turn, {rate} changes a second in all (one change per point each \
         {:.1} s). A general interrogation starts {every_ms} ms after the previous one ended.",
        f64::from(changing) / f64::from(rate)
    );
    println!();
    println!("{}", stats::HEADER);
    println!(
        "{}",
        stats::line("general interrogation", &outcome.gi_times)
    );
    println!(
        "{}",
        stats::line(
            "spontaneous change, from the change to its reception",
            &outcome.change_latencies
        )
    );
    println!();
    println!(
        "- Interrogations: {} completed. Information objects per interrogation: min {min}, max {max} \
         (expected {expected} each).",
        outcome.gi_counts.len()
    );
    println!(
        "- Changes: {sent_changes} made, {} received before the end of the run (the others were in flight).",
        outcome.change_latencies.len()
    );
    println!(
        "- Verdict: {}",
        if interrogations_ok && !outcome.gi_times.is_empty() {
            "PASS: every interrogation answered every point"
        } else {
            "FAIL: an interrogation did not answer every point, or none completed"
        }
    );
    Ok(interrogations_ok && !outcome.gi_times.is_empty())
}

/// Requests the start of the data transfer, and waits for its confirmation.
pub async fn start_transfer(client: &mut Client) -> Result<(), Box<dyn Error>> {
    client.start_data_transfer().await?;
    loop {
        match client.next_event().await {
            Some(Event::Delivery(Delivery::Transfer(TransferState::Started))) => return Ok(()),
            Some(Event::Disconnected(error)) => return Err(format!("disconnected: {error}").into()),
            Some(_) => {}
            None => return Err("the client stopped".into()),
        }
    }
}

/// What the measurement of the load collected.
struct Outcome {
    gi_times: Vec<Duration>,
    gi_counts: Vec<usize>,
    /// The points an interrogation did not answer, for each one that was short.
    gi_missing: Vec<Vec<u32>>,
    change_latencies: Vec<Duration>,
}

/// Interrogates the station every `every` until `end`, and times each interrogation and each change.
async fn measure(
    client: &mut Client,
    sent: &Mutex<Vec<Instant>>,
    end: Instant,
    every: Duration,
    points: u32,
) -> Result<Outcome, Box<dyn Error>> {
    let mut outcome = Outcome {
        gi_times: Vec::new(),
        gi_counts: Vec::new(),
        gi_missing: Vec::new(),
        change_latencies: Vec::new(),
    };
    let mut pending: Option<(Instant, BTreeSet<u32>)> = None;
    let mut next = TokioInstant::now();
    let end = TokioInstant::from_std(end);
    loop {
        tokio::select! {
            () = sleep_until(next), if pending.is_none() => {
                client.interrogate(Qoi::STATION).await?;
                pending = Some((Instant::now(), BTreeSet::new()));
            }
            event = client.next_event() => {
                let Some(event) = event else {
                    return Err("the client stopped".into());
                };
                let Event::Delivery(Delivery::Asdu(asdu)) = event else {
                    continue;
                };
                let now = Instant::now();
                match asdu.cot.cause() {
                    cause::ACTIVATION_TERMINATION if matches!(asdu.body, Body::C_IC_NA_1(_)) => {
                        if let Some((started, answered)) = pending.take() {
                            outcome.gi_times.push(now.saturating_duration_since(started));
                            outcome.gi_counts.push(answered.len());
                            let missing: Vec<u32> = (1..=points)
                                .filter(|number| !answered.contains(number))
                                .take(10)
                                .collect();
                            if !missing.is_empty() {
                                outcome.gi_missing.push(missing);
                            }
                            if let Some(at) = TokioInstant::now().checked_add(every) {
                                next = at;
                            }
                        }
                    }
                    cause::INTERROGATED_STATION => {
                        if let Some((_, answered)) = pending.as_mut() {
                            answered.extend(point_addresses(&asdu));
                        }
                    }
                    cause::SPONTANEOUS => {
                        let made = lock(sent);
                        for value in floats(&asdu) {
                            let index = (value as usize).saturating_sub(1);
                            if let Some(instant) = made.get(index) {
                                outcome.change_latencies.push(now.saturating_duration_since(*instant));
                            }
                        }
                    }
                    _ => {}
                }
            }
            () = sleep_until(end) => break,
        }
    }
    Ok(outcome)
}

/// The information object addresses of an ASDU of M_ME_NC_1.
fn point_addresses(asdu: &Asdu) -> Vec<u32> {
    match &asdu.body {
        Body::M_ME_NC_1(Objects::Individual(items)) => {
            items.iter().map(|item| item.address.value()).collect()
        }
        _ => Vec::new(),
    }
}

/// The float values of an ASDU of M_ME_NC_1.
fn floats(asdu: &Asdu) -> Vec<f32> {
    match &asdu.body {
        Body::M_ME_NC_1(Objects::Individual(items)) => {
            items.iter().map(|item| item.value.0.as_f32()).collect()
        }
        _ => Vec::new(),
    }
}

/// Makes the changes until `end`, on the first `changing` points in turn, `rate` a second. The value
/// of the n-th change is n, and its instant is kept at index n - 1. Returns the number of changes.
async fn drive(
    handle: ServerHandle,
    changing: u32,
    rate: u32,
    sent: Arc<Mutex<Vec<Instant>>>,
    end: Instant,
) -> Result<usize, String> {
    let mut tick = interval(TICK);
    let per_tick = usize::try_from(rate / 100).map_err(|e| e.to_string())?;
    let mut next_point: u32 = 1;
    let mut seq: u32 = 0;
    while Instant::now() < end {
        tick.tick().await;
        for _ in 0..per_tick {
            seq = seq.saturating_add(1);
            let address = InformationObjectAddress::new(next_point).ok_or("an address")?;
            next_point = if next_point >= changing {
                1
            } else {
                next_point.saturating_add(1)
            };
            let value = PointValue::Float(
                ShortFloat::from_f32(seq as f32),
                quality().map_err(|e| e.to_string())?,
            );
            lock(&sent).push(Instant::now());
            handle
                .update(address, value, time().map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(seq as usize)
}

fn lock(sent: &Mutex<Vec<Instant>>) -> MutexGuard<'_, Vec<Instant>> {
    sent.lock().unwrap_or_else(PoisonError::into_inner)
}
