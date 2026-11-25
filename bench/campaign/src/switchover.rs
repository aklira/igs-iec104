// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! The switchover campaign: two stations of one redundancy group, under load. The connection that
//! carries the data is cut during the load, and the controlling station reconnects and starts the
//! data transfer again.
//!
//! The controlling station connects through a TCP relay on loopback, which forwards the bytes to the
//! listener. The cut closes the relayed connection on both sides: the station sees its peer close,
//! and the controlling station sees its connection end, after every delivery of that connection
//! (the client's contract), then reconnects through the relay.
//!
//! Each point of each station changes in turn, and the value of its n-th change is n, so the campaign
//! knows what the station made. The checks, per point, over both connections:
//! - no change is lost: every change the station made is received by the end;
//! - the changes arrive in order: a first delivery never comes after a later change;
//! - a change is delivered at most twice: once, and once more as a resend (§10.5, §10.6).

use std::collections::BTreeMap;
use std::error::Error;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use igs_iec104::client::{Client, ClientConfig, Event};
use igs_iec104::process_image::PointValue;
use igs_iec104::server::ServerHandle;
use igs_iec104::Delivery;
use igs_iec104_codec::asdu::{Asdu, Body, Objects};
use igs_iec104_codec::elements::ShortFloat;
use igs_iec104_codec::header::{cause, CommonAddress, InformationObjectAddress};
use tokio::io::copy_bidirectional;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{watch, Notify};
use tokio::time::{interval, sleep_until, Instant as TokioInstant};

use crate::{option, quality, start, time, SECOND, STATION};

/// The changes are made every 10 ms: `rate / 100` of them each time.
const TICK: Duration = Duration::from_millis(10);
/// The longest the campaign waits for the changes to arrive, after they stop.
const DRAIN: Duration = Duration::from_secs(15);

/// A point of a station: its common address and its information object address.
type Key = (u16, u32);

/// What the campaign checks, over the deliveries of both connections.
#[derive(Default)]
struct Checker {
    /// The last value received for each point.
    last: BTreeMap<Key, u32>,
    /// The number of deliveries of each value of each point.
    deliveries: BTreeMap<(Key, u32), u32>,
    /// The number of distinct changes received for each point.
    distinct: BTreeMap<Key, u32>,
    /// Changes that were skipped: a later value arrived before them.
    gaps: u64,
    /// Changes that arrived for the first time after a later one.
    reorders: u64,
    /// Deliveries of a change that had been delivered before: resends.
    resent: u64,
    /// Deliveries on the first connection, and on the later ones.
    first: u64,
    later: u64,
}

impl Checker {
    fn observe(&mut self, key: Key, value: u32, connection: u64) {
        if connection == 1 {
            self.first = self.first.saturating_add(1);
        } else {
            self.later = self.later.saturating_add(1);
        }
        let count = self.deliveries.entry((key, value)).or_insert(0);
        *count = count.saturating_add(1);
        let first_delivery = *count == 1;
        if first_delivery {
            let distinct = self.distinct.entry(key).or_insert(0);
            *distinct = distinct.saturating_add(1);
        }
        let last = self.last.entry(key).or_insert(0);
        if value > last.saturating_add(1) {
            let skipped = value.saturating_sub(*last).saturating_sub(1);
            self.gaps = self.gaps.saturating_add(u64::from(skipped));
            *last = value;
        } else if value == last.saturating_add(1) {
            *last = value;
        } else if first_delivery {
            self.reorders = self.reorders.saturating_add(1);
        } else {
            self.resent = self.resent.saturating_add(1);
        }
    }

    /// The number of changes the station made that were not received.
    fn missing(&self, made: &BTreeMap<Key, u32>) -> u64 {
        made.iter()
            .map(|(key, value)| {
                let received = self.distinct.get(key).copied().unwrap_or(0);
                u64::from(value.saturating_sub(received))
            })
            .sum()
    }

    /// The most deliveries of one change.
    fn most_deliveries(&self) -> u32 {
        self.deliveries.values().copied().max().unwrap_or(0)
    }
}

/// The controlling station's side of the campaign: its client, what it has received, and the cut.
struct Link {
    client: Client,
    checker: Checker,
    /// The instant of the cut, and the signal that closes the relayed connection.
    cut: Option<(TokioInstant, Arc<Notify>)>,
    /// The connections the client has been given, and the ends it reported.
    connections: u64,
    disconnects: u64,
}

impl Link {
    /// Receives the deliveries until `until`, makes the cut when its instant comes, and starts the data
    /// transfer on each connection after the first.
    async fn receive(&mut self, until: TokioInstant) -> Result<(), Box<dyn Error>> {
        loop {
            let cut_at = self.cut.as_ref().map(|(at, _)| *at);
            tokio::select! {
                event = self.client.next_event() => {
                    let Some(event) = event else {
                        return Err("the client stopped".into());
                    };
                    match event {
                        Event::Connected => {
                            self.connections = self.connections.saturating_add(1);
                            if self.connections > 1 {
                                self.client.start_data_transfer().await?;
                            }
                        }
                        Event::Disconnected(_) => {
                            self.disconnects = self.disconnects.saturating_add(1);
                        }
                        Event::Delivery(Delivery::Asdu(asdu))
                            if asdu.cot.cause() == cause::SPONTANEOUS =>
                        {
                            observe(&mut self.checker, &asdu, self.connections);
                        }
                        _ => {}
                    }
                }
                () = sleep_until(cut_at.unwrap_or(until)), if cut_at.is_some() => {
                    if let Some((_, cut)) = self.cut.take() {
                        cut.notify_one();
                    }
                }
                () = sleep_until(until) => return Ok(()),
            }
        }
    }
}

/// Runs the campaign with the options in `args`. Returns whether it passed.
pub async fn run(args: &[String]) -> Result<bool, Box<dyn Error>> {
    let points: u32 = option(args, "--points", 1_000)?;
    let rate: u32 = option(args, "--rate", 2_000)?;
    let before: u64 = option(args, "--before", 10)?;
    let after: u64 = option(args, "--after", 10)?;
    if rate < 100 || points == 0 {
        return Err("the rate is at least 100, and there is at least one point".into());
    }

    let (address, handle) = start(STATION, &[SECOND], points).await?;
    let second = handle
        .station(CommonAddress::new(SECOND))
        .ok_or("the second station")?;
    let (stop_tx, stop_rx) = watch::channel(false);
    let driver = tokio::spawn(drive(
        vec![(STATION, handle), (SECOND, second)],
        points,
        rate,
        stop_rx,
    ));

    let cut = Arc::new(Notify::new());
    let relay_address = relay(address, Arc::clone(&cut)).await?;
    let client = Client::connect(ClientConfig::new(
        relay_address,
        CommonAddress::new(STATION),
    ));
    client.start_data_transfer().await?;
    let cut_at = TokioInstant::now()
        .checked_add(Duration::from_secs(before))
        .ok_or("the duration")?;
    let end_at = cut_at
        .checked_add(Duration::from_secs(after))
        .ok_or("the duration")?;
    let mut link = Link {
        client,
        checker: Checker::default(),
        cut: Some((cut_at, cut)),
        connections: 0,
        disconnects: 0,
    };
    link.receive(end_at).await?;

    stop_tx.send_replace(true);
    let made = driver.await??;
    let drained_at = Instant::now().checked_add(DRAIN).ok_or("the duration")?;
    while link.checker.missing(&made) > 0 && Instant::now() < drained_at {
        let slice = TokioInstant::now()
            .checked_add(Duration::from_millis(100))
            .ok_or("the duration")?;
        link.receive(slice).await?;
    }

    let changes: u64 = made.values().map(|value| u64::from(*value)).sum();
    let missing = link.checker.missing(&made);
    let most = link.checker.most_deliveries();
    let checker = &link.checker;
    let passed = checker.gaps == 0 && checker.reorders == 0 && missing == 0 && most <= 2;
    println!("# Switchover campaign");
    println!();
    println!(
        "Two stations of one redundancy group on loopback (common addresses {STATION} and {SECOND}), \
         {points} float points each, changing in turn at {rate} changes a second in all. The connection \
         that carries the data is cut {before} s into the load, through a relay on loopback; the \
         controlling station reconnects, starts the data transfer again, and receives the changes for \
         {after} s more. Then the changes stop, and the connection receives what is left."
    );
    println!();
    println!("| measure | value |");
    println!("| --- | --- |");
    println!("| changes made by the stations | {changes} |");
    println!(
        "| connections the controlling station had | {} |",
        link.connections
    );
    println!(
        "| connections that ended (the cut) | {} |",
        link.disconnects
    );
    println!("| deliveries on the first connection | {} |", checker.first);
    println!(
        "| deliveries on the later connections | {} |",
        checker.later
    );
    println!(
        "| resent changes (delivered a second time) | {} |",
        checker.resent
    );
    println!("| most deliveries of one change | {most} |");
    println!(
        "| gaps (a change skipped, then a later one arrived) | {} |",
        checker.gaps
    );
    println!(
        "| reorders (a first delivery after a later change) | {} |",
        checker.reorders
    );
    println!("| changes not received at the end | {missing} |");
    println!();
    println!(
        "- Verdict: {}",
        if passed {
            "PASS: no change lost, in order, at most one resend per change"
        } else {
            "FAIL: see the measures above"
        }
    );
    Ok(passed)
}

/// Starts a relay on loopback to `station`, whose connection `cut` closes. Returns its address.
async fn relay(station: SocketAddr, cut: Arc<Notify>) -> Result<SocketAddr, Box<dyn Error>> {
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let address = listener.local_addr()?;
    tokio::spawn(async move {
        loop {
            let Ok((mut controlling, _)) = listener.accept().await else {
                return;
            };
            let Ok(mut controlled) = TcpStream::connect(station).await else {
                continue;
            };
            // As the client and the station do (transport::without_delay): Nagle's algorithm would hold
            // the frames back for the acknowledgements, and the relay would measure that delay.
            let _ = controlling.set_nodelay(true);
            let _ = controlled.set_nodelay(true);
            // The relayed connection runs until one side ends, or until the cut closes it.
            tokio::select! {
                _ = copy_bidirectional(&mut controlling, &mut controlled) => {}
                () = cut.notified() => {}
            }
        }
    });
    Ok(address)
}

fn observe(checker: &mut Checker, asdu: &Asdu, connection: u64) {
    if let Body::M_ME_NC_1(Objects::Individual(items)) = &asdu.body {
        for item in items {
            let key = (asdu.common_address.value(), item.address.value());
            let value = item.value.0.as_f32();
            checker.observe(key, value as u32, connection);
        }
    }
}

/// Makes the changes until `stop`, round robin over the points of the stations. Returns the number of
/// changes made of each point.
async fn drive(
    stations: Vec<(u16, ServerHandle)>,
    points: u32,
    rate: u32,
    mut stop: watch::Receiver<bool>,
) -> Result<BTreeMap<Key, u32>, String> {
    let mut tick = interval(TICK);
    let per_tick = usize::try_from(rate / 100).map_err(|e| e.to_string())?;
    let keys: Vec<Key> = stations
        .iter()
        .flat_map(|(common, _)| (1..=points).map(move |number| (*common, number)))
        .collect();
    let mut made: BTreeMap<Key, u32> = BTreeMap::new();
    let mut cursor: usize = 0;
    loop {
        tokio::select! {
            _ = tick.tick() => {}
            _ = stop.changed() => break,
        }
        if *stop.borrow() {
            break;
        }
        for _ in 0..per_tick {
            let key = keys.get(cursor).copied().ok_or("no point to change")?;
            cursor = if cursor.saturating_add(1) >= keys.len() {
                0
            } else {
                cursor.saturating_add(1)
            };
            let count = made.entry(key).or_insert(0);
            *count = count.saturating_add(1);
            let value = PointValue::Float(
                ShortFloat::from_f32(*count as f32),
                quality().map_err(|e| e.to_string())?,
            );
            let station = stations
                .iter()
                .find(|(common, _)| *common == key.0)
                .map(|(_, handle)| handle)
                .ok_or("no station")?;
            let address = InformationObjectAddress::new(key.1).ok_or("an address")?;
            station
                .update(address, value, time().map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(made)
}
