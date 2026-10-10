// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! The fault-injection relay of L5. It sits between the probe's driver and a
//! peer, and forwards every frame of each direction, unless a rule names it:
//! the frame is then dropped, delayed or duplicated. The relay reads frames by
//! their APCI length, so it never splits one.
//!
//! A delay holds back the frames behind the delayed one: the relay forwards in
//! order, so the delays of two frames add up.

use std::io;
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpStream;

/// The start octet of every frame (§5.1 of the APCI).
const START: u8 = 0x68;

/// The direction of a frame: sent toward the peer, or toward the driver.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Toward {
    Peer,
    Driver,
}

/// The kind of frame a rule names. `Any` counts every frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Any,
    Information,
    Supervisory,
    Unnumbered,
}

/// What a rule does to the frames it names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fault {
    Drop,
    Delay(Duration),
    Duplicate,
}

/// A rule: the frames of `kind` going `toward`, counted from 0. It names the
/// frames from index `first` on, `count` of them, or all of them when `count` is
/// `None`. For `Kind::Any` the index counts every frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rule {
    pub toward: Toward,
    pub kind: Kind,
    pub first: usize,
    pub count: Option<usize>,
    pub fault: Fault,
}

/// The kind of a frame, from the bits of its first control octet (§5.1): an I
/// frame has a clear first bit, an S frame ends in 01 and a U frame in 11.
fn kind_of(control: u8) -> Kind {
    if control & 0x01 == 0 {
        Kind::Information
    } else if control & 0x03 == 0x01 {
        Kind::Supervisory
    } else {
        Kind::Unnumbered
    }
}

/// How many frames of each kind have passed in one direction.
#[derive(Default)]
struct Seen {
    any: usize,
    information: usize,
    supervisory: usize,
    unnumbered: usize,
}

impl Seen {
    /// Counts one frame of `kind`. Returns its index among the frames of that
    /// kind and its index among all frames.
    fn count(&mut self, kind: Kind) -> (usize, usize) {
        let total = self.any;
        self.any = total.saturating_add(1);
        let of_kind = match kind {
            Kind::Information => &mut self.information,
            Kind::Supervisory => &mut self.supervisory,
            Kind::Unnumbered | Kind::Any => &mut self.unnumbered,
        };
        let index = *of_kind;
        *of_kind = index.saturating_add(1);
        (index, total)
    }
}

/// The fault of the first rule that names the frame, if any.
fn fault_for(
    rules: &[Rule],
    toward: Toward,
    kind: Kind,
    of_kind: usize,
    total: usize,
) -> Option<Fault> {
    rules.iter().find_map(|rule| {
        if rule.toward != toward {
            return None;
        }
        let index = if rule.kind == Kind::Any {
            total
        } else if rule.kind == kind {
            of_kind
        } else {
            return None;
        };
        let from = index >= rule.first;
        let within = rule
            .count
            .is_none_or(|count| index < rule.first.saturating_add(count));
        (from && within).then_some(rule.fault)
    })
}

/// The next whole frame (start octet, length and body), or `None` at the end of
/// the stream.
async fn read_frame<R: AsyncRead + Unpin>(reader: &mut R) -> io::Result<Option<Vec<u8>>> {
    let mut header = [0u8; 2];
    match reader.read_exact(&mut header).await {
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(error) => return Err(error),
    }
    let [start, length] = header;
    if start != START {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "a frame does not start with 68H",
        ));
    }
    let mut body = vec![0u8; usize::from(length)];
    reader.read_exact(&mut body).await?;
    let mut frame = header.to_vec();
    frame.extend_from_slice(&body);
    Ok(Some(frame))
}

/// Forwards the frames of one direction under `rules`, until the input ends.
async fn pump<R, W>(mut reader: R, mut writer: W, toward: Toward, rules: &[Rule]) -> io::Result<()>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut seen = Seen::default();
    while let Some(frame) = read_frame(&mut reader).await? {
        // A frame with no control octet cannot be an I frame: it is counted as U.
        let kind = kind_of(frame.get(2).copied().unwrap_or(0x03));
        let (of_kind, total) = seen.count(kind);
        match fault_for(rules, toward, kind, of_kind, total) {
            None => writer.write_all(&frame).await?,
            Some(Fault::Drop) => {}
            Some(Fault::Duplicate) => {
                writer.write_all(&frame).await?;
                writer.write_all(&frame).await?;
            }
            Some(Fault::Delay(delay)) => {
                tokio::time::sleep(delay).await;
                writer.write_all(&frame).await?;
            }
        }
    }
    writer.shutdown().await
}

/// Forwards the frames between a driver's stream and a peer's stream under
/// `rules`. The frames the driver sends go toward the peer; the frames the peer
/// sends go toward the driver. Returns when both directions have ended.
pub async fn bridge(driver: TcpStream, peer: TcpStream, rules: &[Rule]) -> io::Result<()> {
    let (driver_read, driver_write) = driver.into_split();
    let (peer_read, peer_write) = peer.into_split();
    let (to_peer, to_driver) = tokio::join!(
        pump(driver_read, peer_write, Toward::Peer, rules),
        pump(peer_read, driver_write, Toward::Driver, rules),
    );
    to_peer?;
    to_driver
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
mod tests {
    use super::*;
    use tokio::io::duplex;

    #[test]
    fn the_control_octet_tells_the_kind_of_frame() {
        assert_eq!(kind_of(0x00), Kind::Information);
        assert_eq!(kind_of(0x0E), Kind::Information);
        assert_eq!(kind_of(0x01), Kind::Supervisory);
        assert_eq!(kind_of(0x07), Kind::Unnumbered);
        assert_eq!(kind_of(0x43), Kind::Unnumbered);
    }

    #[test]
    fn a_rule_names_the_nth_frame_of_its_kind() {
        let rule = Rule {
            toward: Toward::Peer,
            kind: Kind::Information,
            first: 1,
            count: Some(1),
            fault: Fault::Drop,
        };
        let rules = [rule];
        assert_eq!(
            fault_for(&rules, Toward::Peer, Kind::Information, 0, 0),
            None
        );
        assert_eq!(
            fault_for(&rules, Toward::Peer, Kind::Information, 1, 1),
            Some(Fault::Drop)
        );
        assert_eq!(
            fault_for(&rules, Toward::Peer, Kind::Information, 2, 2),
            None
        );
        assert_eq!(
            fault_for(&rules, Toward::Driver, Kind::Information, 1, 1),
            None
        );
        assert_eq!(
            fault_for(&rules, Toward::Peer, Kind::Supervisory, 1, 1),
            None
        );
    }

    #[tokio::test]
    async fn a_dropped_frame_is_not_forwarded_and_the_others_are() {
        // Three I frames with N(S) 0, 1 and 2; the rule drops the second.
        let frames = [
            [0x68, 0x04, 0x00, 0x00, 0x00, 0x00],
            [0x68, 0x04, 0x02, 0x00, 0x00, 0x00],
            [0x68, 0x04, 0x04, 0x00, 0x00, 0x00],
        ];
        let (mut input, reader) = duplex(256);
        let (writer, mut output) = duplex(256);
        for frame in &frames {
            input
                .write_all(frame)
                .await
                .expect("the input accepts the frame");
        }
        drop(input);
        let rules = [Rule {
            toward: Toward::Peer,
            kind: Kind::Information,
            first: 1,
            count: Some(1),
            fault: Fault::Drop,
        }];
        pump(reader, writer, Toward::Peer, &rules)
            .await
            .expect("the pump ends cleanly");
        let mut forwarded = Vec::new();
        output
            .read_to_end(&mut forwarded)
            .await
            .expect("the output is readable");
        let expected: Vec<u8> = [frames[0], frames[2]].concat();
        assert_eq!(forwarded, expected);
    }
}
