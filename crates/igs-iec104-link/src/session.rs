// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! The APCI session state machine (IEC 60870-5-104 §5.1, §5.2, §5.3), sans I/O.
//!
//! The session takes events (bytes received, an ASDU to send, a request to
//! start or stop the data transfer, a tick) and returns actions (frames to
//! send, ASDUs to deliver, closing the connection). It keeps the send and
//! receive state variables, the window of k unacknowledged I frames, the
//! acknowledgement after w received I frames, and the STARTDT and STOPDT states.
//!
//! The controlled station follows figure 17 and the controlling station
//! follows figure 18 of §5.3. Where the text says more than the figure, the
//! text is followed and recorded as a question (Q-008).
//!
//! The timers t1, t2 and t3 are in `timers`. They run on the instants given to
//! `handle`, and `next_deadline` tells the caller when the next one is due.

use std::collections::VecDeque;
use std::time::Instant;

use igs_iec104_codec::apci::{
    Apdu, FrameDecoder, SequenceNumber, UnnumberedFunction, MAX_SEQUENCE_NUMBER,
};
use igs_iec104_codec::asdu::Asdu;
use igs_iec104_codec::error::DecodeError;

use crate::config::{LinkConfig, Role};

mod timers;

/// The state of the data transfer on the connection (§5.3, figures 17 and 18).
///
/// The controlled station only goes through `Stopped`, `Started` and
/// `PendingUnconfirmedStop`. The controlling station also goes through
/// `PendingStarted` and `PendingStopped`.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum TransferState {
    /// STOPDT: no ASDU is sent. The state after the connection is established.
    Stopped,
    /// STARTDT act was sent, and the controlling station waits for STARTDT con.
    PendingStarted,
    /// STARTDT: the data transfer is enabled.
    Started,
    /// STOPDT act was sent, and the controlling station waits for STOPDT con.
    PendingStopped,
    /// STOPDT act was received (controlled) or sent (controlling) while I frames
    /// are still unacknowledged. The stop is confirmed once they are.
    PendingUnconfirmedStop,
}

/// A request of the user, for the start and stop procedures.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Request {
    /// Start the data transfer (STARTDT act).
    StartDt,
    /// Stop the data transfer (STOPDT act).
    StopDt,
}

/// Something that arrived or that the user asked for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// Octets received from the transport. They may hold any number of frames,
    /// and a frame may be split over several events.
    Received(Vec<u8>),
    /// An ASDU to send as an I frame.
    SendAsdu(Asdu),
    /// The user asks to start the data transfer.
    StartDt,
    /// The user asks to stop the data transfer.
    StopDt,
    /// Another connection of the redundancy group starts the data transfer: this one
    /// is closed unless it is stopped (§10.7).
    Supersede,
    /// Time passes: the timers due by now expire. An early tick does nothing.
    Tick,
}

/// Why a user request was not carried out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Rejection {
    /// The window of k unacknowledged I frames is full. The ASDU is returned.
    WindowFull(Asdu),
    /// The data transfer is not started. The ASDU is returned.
    TransferStopped(Asdu),
    /// The request is for the controlling station, and this session is controlled.
    NotForRole(Request),
    /// The request does not apply in the current state.
    InvalidState {
        /// The request.
        request: Request,
        /// The state of the data transfer.
        state: TransferState,
    },
}

/// Why the connection is closed by this station.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CloseReason {
    /// An I frame arrived in a state where the figures expect none (figures 17
    /// and 18).
    UnexpectedIFrame,
    /// An S frame arrived in a state where the figures expect none (figures 17
    /// and 18).
    UnexpectedSFrame,
    /// The N(S) of an I frame is not the next number expected (QUESTIONS.md Q-005).
    SequenceError {
        /// The number expected.
        expected: SequenceNumber,
        /// The number received.
        received: SequenceNumber,
    },
    /// The N(R) acknowledges frames that were never sent (QUESTIONS.md Q-003).
    InvalidReceiveSequence {
        /// The N(R) received.
        receive: SequenceNumber,
    },
    /// A frame could not be decoded, or the framing failed (QUESTIONS.md Q-006).
    Decode(DecodeError),
    /// t1 expired while a frame, a start or stop act, or a test act was waiting
    /// for its confirmation (§5.1, §5.2).
    T1Expired,
    /// Another connection of the redundancy group started the data transfer (§10.7).
    Superseded,
}

/// What the session asks the caller to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    /// Send this frame to the peer.
    Send(Apdu),
    /// Deliver this ASDU to the application.
    Deliver(Asdu),
    /// A user request was not carried out.
    Rejected(Rejection),
    /// The transfer state changed to this state (§5.3). It follows the frames of the transition,
    /// so the application sees the change in order with the ASDUs that come after it.
    Transfer(TransferState),
    /// Close the connection. The session accepts no more events after it.
    Close(CloseReason),
}

/// The APCI state of one connection.
#[derive(Debug)]
pub struct Session {
    config: LinkConfig,
    decoder: FrameDecoder,
    transfer: TransferState,
    closed: bool,
    /// V(S): the N(S) of the next I frame to send.
    send_state: SequenceNumber,
    /// V(R): the N(S) of the next I frame expected from the peer.
    receive_state: SequenceNumber,
    /// The N(R) last received: the peer has received every I frame before it.
    acknowledged: SequenceNumber,
    /// The N(R) last sent: every I frame of the peer before it is acknowledged.
    acknowledged_to_peer: SequenceNumber,
    /// The ASDUs of the I frames sent and not yet acknowledged, oldest first. Their number
    /// is `outstanding()`. A redundancy group sends them again on another connection when
    /// this one is closed before they are acknowledged (§10.5, §10.6).
    unacknowledged: VecDeque<Asdu>,
    last_received: Option<Instant>,
    /// t1: when the last frame or act sent expires, unless it is confirmed.
    t1_deadline: Option<Instant>,
    /// t2: when the acknowledgement of the received I frames expires.
    t2_deadline: Option<Instant>,
    /// t3: when the connection has been idle long enough to be tested.
    t3_deadline: Option<Instant>,
    /// True while a TESTFR act waits for its TESTFR con.
    test_pending: bool,
}

/// The number of steps from `from` to `to` modulo 32768.
fn distance(from: SequenceNumber, to: SequenceNumber) -> u16 {
    to.value().wrapping_sub(from.value()) & MAX_SEQUENCE_NUMBER
}

impl Session {
    /// A session for a connection established at `now`: the sequence numbers are
    /// zero, the data transfer is stopped (§5.1, §5.3) and t3 starts (§5.2).
    pub fn new(config: LinkConfig, now: Instant) -> Self {
        let t3_deadline = timers::deadline(now, config.parameters().t3);
        Self {
            config,
            decoder: FrameDecoder::new(),
            transfer: TransferState::Stopped,
            closed: false,
            send_state: SequenceNumber::ZERO,
            receive_state: SequenceNumber::ZERO,
            acknowledged: SequenceNumber::ZERO,
            acknowledged_to_peer: SequenceNumber::ZERO,
            unacknowledged: VecDeque::new(),
            last_received: None,
            t1_deadline: None,
            t2_deadline: None,
            t3_deadline,
            test_pending: false,
        }
    }

    /// The state of the data transfer.
    pub fn transfer(&self) -> TransferState {
        self.transfer
    }

    /// True once the session has asked to close the connection.
    pub fn is_closed(&self) -> bool {
        self.closed
    }

    /// The number of I frames sent and not yet acknowledged by the peer.
    pub fn outstanding(&self) -> u16 {
        distance(self.acknowledged, self.send_state)
    }

    /// The ASDUs of the I frames sent and not yet acknowledged, oldest first. The
    /// caller sends them again on another connection after this one is closed.
    pub fn unacknowledged(&self) -> Vec<Asdu> {
        self.unacknowledged.iter().cloned().collect()
    }

    /// The time of the last frame received, if any.
    pub fn last_received(&self) -> Option<Instant> {
        self.last_received
    }

    /// Handles one event at time `now` and returns the actions to carry out.
    pub fn handle(&mut self, event: Event, now: Instant) -> Vec<Action> {
        if self.closed {
            return Vec::new();
        }
        let mut actions = Vec::new();
        self.expire_timers(now, &mut actions);
        if self.closed {
            return actions;
        }
        match event {
            Event::Received(bytes) => actions.extend(self.receive(&bytes, now)),
            Event::SendAsdu(asdu) => actions.push(self.send_asdu(asdu, now)),
            Event::StartDt => actions.extend(self.request(Request::StartDt, now)),
            Event::StopDt => actions.extend(self.request(Request::StopDt, now)),
            Event::Supersede => self.supersede(&mut actions),
            Event::Tick => {}
        }
        self.sync_t1();
        actions
    }

    fn receive(&mut self, bytes: &[u8], now: Instant) -> Vec<Action> {
        let mut actions = Vec::new();
        let mut input = bytes;
        loop {
            let frame = match self.decoder.next_frame(&mut input) {
                Ok(Some(frame)) => frame,
                Ok(None) => return actions,
                Err(error) => {
                    self.close(CloseReason::Decode(error), &mut actions);
                    return actions;
                }
            };
            self.frame_received(now);
            let outcome = match Apdu::decode(&frame) {
                Ok(apdu) => self.apply(apdu, now, &mut actions),
                Err(error) => Err(CloseReason::Decode(error)),
            };
            if let Err(reason) = outcome {
                self.close(reason, &mut actions);
                return actions;
            }
        }
    }

    /// Applies one received frame. An error is the reason to close.
    fn apply(
        &mut self,
        apdu: Apdu,
        now: Instant,
        out: &mut Vec<Action>,
    ) -> Result<(), CloseReason> {
        match apdu {
            Apdu::Information {
                send,
                receive,
                asdu,
            } => self.information(send, receive, asdu, now, out),
            Apdu::Supervisory { receive } => self.supervisory(receive, out),
            Apdu::Unnumbered(function) => {
                self.unnumbered(function, out);
                Ok(())
            }
        }
    }

    fn information(
        &mut self,
        send: SequenceNumber,
        receive: SequenceNumber,
        asdu: Asdu,
        now: Instant,
        out: &mut Vec<Action>,
    ) -> Result<(), CloseReason> {
        if !self.accepts_i_frames() {
            return Err(CloseReason::UnexpectedIFrame);
        }
        self.acknowledge(receive)?;
        if send != self.receive_state {
            return Err(CloseReason::SequenceError {
                expected: self.receive_state,
                received: send,
            });
        }
        self.receive_state = self.receive_state.next();
        out.push(Action::Deliver(asdu));
        self.settle_stop(out);
        if self.is_stop_pending_on_controlling_station() {
            // §5.3: the controlling station answers at once while the stop is pending.
            self.send_supervisory(out);
        } else if distance(self.acknowledged_to_peer, self.receive_state)
            >= self.config.parameters().w
        {
            // §5.5: the acknowledgement is due at the latest after w I frames.
            self.send_supervisory(out);
        }
        self.start_t2_if_unacknowledged(now);
        Ok(())
    }

    fn supervisory(
        &mut self,
        receive: SequenceNumber,
        out: &mut Vec<Action>,
    ) -> Result<(), CloseReason> {
        // Figures 17 and 18: no S frame while stopped, or while the start is pending.
        if matches!(
            self.transfer,
            TransferState::Stopped | TransferState::PendingStarted
        ) {
            return Err(CloseReason::UnexpectedSFrame);
        }
        self.acknowledge(receive)?;
        self.settle_stop(out);
        Ok(())
    }

    /// Takes the N(R) of a received frame as the acknowledgement of the frames
    /// sent. It is valid when it lies between the last acknowledged number and
    /// V(S) (§5.1).
    fn acknowledge(&mut self, receive: SequenceNumber) -> Result<(), CloseReason> {
        let count = distance(self.acknowledged, receive);
        if count > self.outstanding() {
            return Err(CloseReason::InvalidReceiveSequence { receive });
        }
        // The frames acknowledged are the oldest ones sent: their ASDUs leave the queue.
        for _ in 0..count {
            self.unacknowledged.pop_front();
        }
        self.acknowledged = receive;
        Ok(())
    }

    /// Completes a pending stop once every sent I frame is acknowledged: the
    /// controlled station confirms it (figure 17), the controlling station waits
    /// for STOPDT con (figure 18).
    fn settle_stop(&mut self, out: &mut Vec<Action>) {
        if self.transfer != TransferState::PendingUnconfirmedStop || self.outstanding() != 0 {
            return;
        }
        match self.config.role() {
            Role::Controlled => {
                out.push(Action::Send(Apdu::Unnumbered(
                    UnnumberedFunction::StopDtCon,
                )));
                self.set_transfer(TransferState::Stopped, out);
            }
            Role::Controlling => self.set_transfer(TransferState::PendingStopped, out),
        }
    }

    /// Sets the transfer state. A change is reported as an action, after the frames of the same
    /// transition, so that it stays in order with the ASDUs of the same read.
    fn set_transfer(&mut self, state: TransferState, out: &mut Vec<Action>) {
        if self.transfer != state {
            self.transfer = state;
            out.push(Action::Transfer(state));
        }
    }

    /// True when an I frame may arrive in the current state. Figure 17 closes on
    /// any I frame once the stop is pending; figure 18 does not.
    fn accepts_i_frames(&self) -> bool {
        match self.transfer {
            TransferState::Started => true,
            TransferState::PendingStopped | TransferState::PendingUnconfirmedStop => {
                self.config.role() == Role::Controlling
            }
            TransferState::Stopped | TransferState::PendingStarted => false,
        }
    }

    fn is_stop_pending_on_controlling_station(&self) -> bool {
        self.config.role() == Role::Controlling
            && matches!(
                self.transfer,
                TransferState::PendingStopped | TransferState::PendingUnconfirmedStop
            )
    }

    fn unnumbered(&mut self, function: UnnumberedFunction, out: &mut Vec<Action>) {
        // §5.2: every test frame is confirmed, in any state.
        if function == UnnumberedFunction::TestFrAct {
            out.push(Action::Send(Apdu::Unnumbered(
                UnnumberedFunction::TestFrCon,
            )));
            return;
        }
        // §5.2: a confirmation ends the wait for the test act. Without a wait,
        // it changes nothing (QUESTIONS.md Q-011).
        if function == UnnumberedFunction::TestFrCon {
            self.test_pending = false;
            return;
        }
        match (self.config.role(), function, self.transfer) {
            // Figure 17: the controlled station starts and stops.
            (Role::Controlled, UnnumberedFunction::StartDtAct, TransferState::Stopped) => {
                out.push(Action::Send(Apdu::Unnumbered(
                    UnnumberedFunction::StartDtCon,
                )));
                self.set_transfer(TransferState::Started, out);
            }
            (Role::Controlled, UnnumberedFunction::StopDtAct, TransferState::Started) => {
                self.controlled_stop(out);
            }
            // Figure 18: the controlling station is confirmed.
            (Role::Controlling, UnnumberedFunction::StartDtCon, TransferState::PendingStarted) => {
                self.set_transfer(TransferState::Started, out);
            }
            (
                Role::Controlling,
                UnnumberedFunction::StopDtCon,
                TransferState::PendingStopped | TransferState::PendingUnconfirmedStop,
            ) => {
                self.set_transfer(TransferState::Stopped, out);
            }
            // Any other U frame leaves the state unchanged (figures 17 and 18).
            _ => {}
        }
    }

    /// The controlled station receives STOPDT act while started (figure 17).
    fn controlled_stop(&mut self, out: &mut Vec<Action>) {
        // §5.3: confirm the frames received before the stop, then wait for the
        // acknowledgements of the frames sent, or confirm the stop at once.
        if distance(self.acknowledged_to_peer, self.receive_state) > 0 {
            self.send_supervisory(out);
        }
        if self.outstanding() > 0 {
            self.set_transfer(TransferState::PendingUnconfirmedStop, out);
        } else {
            out.push(Action::Send(Apdu::Unnumbered(
                UnnumberedFunction::StopDtCon,
            )));
            self.set_transfer(TransferState::Stopped, out);
        }
    }

    fn send_asdu(&mut self, asdu: Asdu, now: Instant) -> Action {
        if self.transfer != TransferState::Started {
            return Action::Rejected(Rejection::TransferStopped(asdu));
        }
        if self.outstanding() >= self.config.parameters().k {
            return Action::Rejected(Rejection::WindowFull(asdu));
        }
        self.unacknowledged.push_back(asdu.clone());
        let frame = Apdu::Information {
            send: self.send_state,
            receive: self.receive_state,
            asdu,
        };
        self.send_state = self.send_state.next();
        self.mark_acknowledged_to_peer();
        self.arm_t1(now);
        Action::Send(frame)
    }

    fn send_supervisory(&mut self, out: &mut Vec<Action>) {
        out.push(Action::Send(Apdu::Supervisory {
            receive: self.receive_state,
        }));
        self.mark_acknowledged_to_peer();
    }

    /// The user asks to start or stop the data transfer. Only the controlling
    /// station asks; the controlled station answers the peer's requests.
    fn request(&mut self, request: Request, now: Instant) -> Vec<Action> {
        match (self.config.role(), request, self.transfer) {
            (Role::Controlled, _, _) => {
                vec![Action::Rejected(Rejection::NotForRole(request))]
            }
            // Figure 18: Start connection / send STARTDT act.
            (Role::Controlling, Request::StartDt, TransferState::Stopped) => {
                self.arm_t1(now);
                let mut actions = vec![Action::Send(Apdu::Unnumbered(
                    UnnumberedFunction::StartDtAct,
                ))];
                self.set_transfer(TransferState::PendingStarted, &mut actions);
                actions
            }
            // Figure 18: Stop connection / send STOPDT act. The frames received
            // are confirmed first (§5.3), then the act is sent.
            (Role::Controlling, Request::StopDt, TransferState::Started) => {
                let mut actions = Vec::new();
                if distance(self.acknowledged_to_peer, self.receive_state) > 0 {
                    self.send_supervisory(&mut actions);
                }
                self.arm_t1(now);
                actions.push(Action::Send(Apdu::Unnumbered(
                    UnnumberedFunction::StopDtAct,
                )));
                let pending = if self.outstanding() > 0 {
                    TransferState::PendingUnconfirmedStop
                } else {
                    TransferState::PendingStopped
                };
                self.set_transfer(pending, &mut actions);
                actions
            }
            (Role::Controlling, _, state) => {
                vec![Action::Rejected(Rejection::InvalidState { request, state })]
            }
        }
    }

    /// Another connection of the redundancy group starts the data transfer, so this one
    /// is closed (§10.7). Figure 36 closes a started controlled connection, and figure 37 a
    /// started or starting controlling one. §10.7 also closes a controlled connection whose
    /// stop is pending for its acknowledgements; figure 36 has no such arrow (Q-017). A
    /// stopped connection is not closed.
    fn supersede(&mut self, out: &mut Vec<Action>) {
        let closes = matches!(
            (self.config.role(), self.transfer),
            (_, TransferState::Started)
                | (Role::Controlling, TransferState::PendingStarted)
                | (Role::Controlled, TransferState::PendingUnconfirmedStop)
        );
        if closes {
            self.close(CloseReason::Superseded, out);
        }
    }

    fn close(&mut self, reason: CloseReason, out: &mut Vec<Action>) {
        self.closed = true;
        out.push(Action::Close(reason));
    }
}

#[cfg(test)]
mod tests;
