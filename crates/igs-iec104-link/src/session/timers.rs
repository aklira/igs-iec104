// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! The timers of the session (IEC 60870-5-104 §5.1, §5.2 and §9.6), sans I/O.
//!
//! - t1, time-out of send or test APDUs: started by the last I frame, start act,
//!   stop act or test act sent. It runs while anything awaits its confirmation,
//!   and its expiry closes the connection.
//! - t2, time-out for acknowledgements when there is no data: started by a
//!   received I frame that is not acknowledged yet. Its expiry sends an S frame.
//! - t3, time-out for test frames in a long idle state: restarted by every frame
//!   received. Its expiry sends a TESTFR act.
//!
//! The session has no clock. Each call to `handle` first runs the timers due at
//! its instant, then the event. `next_deadline` tells the caller when the next
//! timer is due.

use std::time::{Duration, Instant};

use igs_iec104_codec::apci::{Apdu, UnnumberedFunction};

use super::{distance, Action, CloseReason, Session, TransferState};

/// The instant `timeout` after `now`. None when the clock cannot hold it: the
/// timer then never expires.
pub(super) fn deadline(now: Instant, timeout: Duration) -> Option<Instant> {
    now.checked_add(timeout)
}

impl Session {
    /// The instant at which the next timer expires, if one runs. The caller
    /// calls `handle` with `Event::Tick` at or after it; an earlier tick does
    /// nothing.
    pub fn next_deadline(&self) -> Option<Instant> {
        if self.closed {
            return None;
        }
        [self.t1_deadline, self.t2_deadline, self.t3_deadline]
            .into_iter()
            .flatten()
            .min()
    }

    /// Runs the timers due at `now`. An expired t1 closes the connection; an
    /// expired t2 sends the acknowledgement that is due; an expired t3 asks the
    /// peer for a test.
    pub(super) fn expire_timers(&mut self, now: Instant, out: &mut Vec<Action>) {
        if self.t1_deadline.is_some_and(|due| due <= now) {
            self.t1_deadline = None;
            self.close(CloseReason::T1Expired, out);
            return;
        }
        if self.t2_deadline.is_some_and(|due| due <= now) {
            self.t2_deadline = None;
            if distance(self.acknowledged_to_peer, self.receive_state) > 0 {
                self.send_supervisory(out);
            }
        }
        if self.t3_deadline.is_some_and(|due| due <= now) {
            self.t3_deadline = deadline(now, self.config.parameters().t3);
            // §5.2: one test at a time. A second test waits for the confirmation of
            // the first one.
            if !self.test_pending {
                self.test_pending = true;
                self.arm_t1(now);
                out.push(Action::Send(Apdu::Unnumbered(
                    UnnumberedFunction::TestFrAct,
                )));
            }
        }
    }

    /// Starts t1 for the frame or the act just sent.
    pub(super) fn arm_t1(&mut self, now: Instant) {
        self.t1_deadline = deadline(now, self.config.parameters().t1);
    }

    /// Stops t1 once nothing awaits its confirmation: every sent I frame is
    /// acknowledged, no start or stop act is pending and no test act is unconfirmed.
    pub(super) fn sync_t1(&mut self) {
        let awaiting = self.outstanding() > 0
            || self.test_pending
            || matches!(
                self.transfer,
                TransferState::PendingStarted
                    | TransferState::PendingStopped
                    | TransferState::PendingUnconfirmedStop
            );
        if !awaiting {
            self.t1_deadline = None;
        }
    }

    /// Every frame received restarts t3 (§5.2).
    pub(super) fn frame_received(&mut self, now: Instant) {
        self.last_received = Some(now);
        self.t3_deadline = deadline(now, self.config.parameters().t3);
    }

    /// Starts t2 for a received I frame that is not acknowledged yet. Later
    /// frames do not restart it: the acknowledgement is due t2 after the first.
    pub(super) fn start_t2_if_unacknowledged(&mut self, now: Instant) {
        if self.t2_deadline.is_none() && distance(self.acknowledged_to_peer, self.receive_state) > 0
        {
            self.t2_deadline = deadline(now, self.config.parameters().t2);
        }
    }

    /// The peer holds every received I frame: the N(R) sent is V(R), and no
    /// acknowledgement is due.
    pub(super) fn mark_acknowledged_to_peer(&mut self) {
        self.acknowledged_to_peer = self.receive_state;
        self.t2_deadline = None;
    }
}
