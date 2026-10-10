// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! Deterministic tests of the session with a fake clock: each event carries
//! the instant it happens at, and no test reads the real clock.

use std::time::{Duration, Instant};

use igs_iec104_codec::apci::{Apdu, SequenceNumber, UnnumberedFunction};
use igs_iec104_codec::asdu::Asdu;
use igs_iec104_codec::error::DecodeError;

use super::*;
use crate::config::{LinkConfig, Parameters, Role};

/// C_IC_NA_1, activation, station interrogation: the ASDU of every I frame here.
const INTERROGATION: &[u8] = &[0x64, 0x01, 0x06, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x14];

fn sn(value: u16) -> SequenceNumber {
    SequenceNumber::new(value).expect("in range")
}

fn asdu() -> Asdu {
    Asdu::decode(INTERROGATION).expect("sample decodes")
}

/// The instant `seconds` after the start of the test.
fn at(base: Instant, seconds: u64) -> Instant {
    base + Duration::from_secs(seconds)
}

fn bytes(apdu: Apdu) -> Vec<u8> {
    apdu.to_vec().expect("sample encodes")
}

/// An I frame from the peer, with its N(S) and its N(R).
fn information(send: u16, receive: u16) -> Vec<u8> {
    bytes(Apdu::Information {
        send: sn(send),
        receive: sn(receive),
        asdu: asdu(),
    })
}

fn supervisory(receive: u16) -> Vec<u8> {
    bytes(Apdu::Supervisory {
        receive: sn(receive),
    })
}

fn unnumbered(function: UnnumberedFunction) -> Vec<u8> {
    bytes(Apdu::Unnumbered(function))
}

fn send_frame(send: u16, receive: u16) -> Action {
    Action::Send(Apdu::Information {
        send: sn(send),
        receive: sn(receive),
        asdu: asdu(),
    })
}

fn send_s(receive: u16) -> Action {
    Action::Send(Apdu::Supervisory {
        receive: sn(receive),
    })
}

fn send_u(function: UnnumberedFunction) -> Action {
    Action::Send(Apdu::Unnumbered(function))
}

fn controlled(now: Instant) -> Session {
    Session::new(LinkConfig::with_defaults(Role::Controlled), now)
}

fn controlled_with(parameters: Parameters, now: Instant) -> Session {
    Session::new(
        LinkConfig::new(Role::Controlled, parameters).expect("valid parameters"),
        now,
    )
}

/// A controlled session whose peer has started the data transfer.
fn started(base: Instant) -> Session {
    let mut session = controlled(base);
    let actions = session.handle(
        Event::Received(unnumbered(UnnumberedFunction::StartDtAct)),
        at(base, 0),
    );
    assert_eq!(actions, vec![send_u(UnnumberedFunction::StartDtCon)]);
    assert_eq!(session.transfer(), TransferState::Started);
    session
}

fn receive(session: &mut Session, frames: Vec<u8>, now: Instant) -> Vec<Action> {
    session.handle(Event::Received(frames), now)
}

fn send(session: &mut Session, asdu: Asdu, now: Instant) -> Vec<Action> {
    session.handle(Event::SendAsdu(asdu), now)
}

#[test]
fn a_new_session_is_stopped_with_zero_sequence_numbers() {
    let base = Instant::now();
    let session = controlled(base);
    assert_eq!(session.transfer(), TransferState::Stopped);
    assert_eq!(session.outstanding(), 0);
    assert!(!session.is_closed());
    assert_eq!(session.last_received(), None);
}

#[test]
fn startdt_act_is_confirmed_and_starts_the_transfer() {
    let base = Instant::now();
    let mut session = controlled(base);
    let actions = receive(
        &mut session,
        unnumbered(UnnumberedFunction::StartDtAct),
        at(base, 1),
    );
    assert_eq!(actions, vec![send_u(UnnumberedFunction::StartDtCon)]);
    assert_eq!(session.transfer(), TransferState::Started);
}

#[test]
fn startdt_act_in_the_started_state_changes_nothing() {
    let base = Instant::now();
    let mut session = started(base);
    let actions = receive(
        &mut session,
        unnumbered(UnnumberedFunction::StartDtAct),
        at(base, 1),
    );
    assert!(actions.is_empty());
    assert_eq!(session.transfer(), TransferState::Started);
}

#[test]
fn stopdt_act_in_the_stopped_state_changes_nothing() {
    // Figure 17 has no transition for it: only the U-frame loop of the stopped state.
    let base = Instant::now();
    let mut session = controlled(base);
    let actions = receive(
        &mut session,
        unnumbered(UnnumberedFunction::StopDtAct),
        at(base, 1),
    );
    assert!(actions.is_empty());
    assert_eq!(session.transfer(), TransferState::Stopped);
}

#[test]
fn an_i_frame_while_stopped_closes_the_connection() {
    let base = Instant::now();
    let mut session = controlled(base);
    let actions = receive(&mut session, information(0, 0), at(base, 1));
    assert_eq!(actions, vec![Action::Close(CloseReason::UnexpectedIFrame)]);
    assert!(session.is_closed());
}

#[test]
fn an_s_frame_while_stopped_closes_the_connection() {
    let base = Instant::now();
    let mut session = controlled(base);
    let actions = receive(&mut session, supervisory(0), at(base, 1));
    assert_eq!(actions, vec![Action::Close(CloseReason::UnexpectedSFrame)]);
}

#[test]
fn test_frames_are_confirmed_in_every_state() {
    let base = Instant::now();
    let mut session = controlled(base);
    let actions = receive(
        &mut session,
        unnumbered(UnnumberedFunction::TestFrAct),
        at(base, 1),
    );
    assert_eq!(actions, vec![send_u(UnnumberedFunction::TestFrCon)]);
    assert_eq!(session.transfer(), TransferState::Stopped);
    assert!(!session.is_closed());
}

#[test]
fn stopdt_act_without_unconfirmed_frames_is_confirmed_at_once() {
    let base = Instant::now();
    let mut session = started(base);
    let actions = receive(
        &mut session,
        unnumbered(UnnumberedFunction::StopDtAct),
        at(base, 1),
    );
    assert_eq!(actions, vec![send_u(UnnumberedFunction::StopDtCon)]);
    assert_eq!(session.transfer(), TransferState::Stopped);
}

#[test]
fn stopdt_act_confirms_the_received_frames_before_the_stop() {
    let base = Instant::now();
    let mut session = started(base);
    // One I frame received: not yet acknowledged (w is 8).
    let delivered = receive(&mut session, information(0, 0), at(base, 1));
    assert_eq!(delivered, vec![Action::Deliver(asdu())]);
    let actions = receive(
        &mut session,
        unnumbered(UnnumberedFunction::StopDtAct),
        at(base, 2),
    );
    assert_eq!(
        actions,
        vec![send_s(1), send_u(UnnumberedFunction::StopDtCon)]
    );
    assert_eq!(session.transfer(), TransferState::Stopped);
}

#[test]
fn stopdt_act_waits_for_the_acknowledgement_of_sent_frames() {
    let base = Instant::now();
    let mut session = started(base);
    assert_eq!(
        send(&mut session, asdu(), at(base, 1)),
        vec![send_frame(0, 0)]
    );
    assert_eq!(
        send(&mut session, asdu(), at(base, 1)),
        vec![send_frame(1, 0)]
    );

    let actions = receive(
        &mut session,
        unnumbered(UnnumberedFunction::StopDtAct),
        at(base, 2),
    );
    assert!(
        actions.is_empty(),
        "no confirmation before the acknowledgement"
    );
    assert_eq!(session.transfer(), TransferState::PendingUnconfirmedStop);

    // The peer acknowledges one frame: still pending.
    let partial = receive(&mut session, supervisory(1), at(base, 3));
    assert!(partial.is_empty());
    assert_eq!(session.transfer(), TransferState::PendingUnconfirmedStop);

    // Then the second: the stop is confirmed.
    let confirmed = receive(&mut session, supervisory(2), at(base, 4));
    assert_eq!(confirmed, vec![send_u(UnnumberedFunction::StopDtCon)]);
    assert_eq!(session.transfer(), TransferState::Stopped);
    assert_eq!(session.outstanding(), 0);
}

#[test]
fn asdus_are_not_sent_while_the_stop_is_pending() {
    let base = Instant::now();
    let mut session = started(base);
    send(&mut session, asdu(), at(base, 1));
    receive(
        &mut session,
        unnumbered(UnnumberedFunction::StopDtAct),
        at(base, 2),
    );
    let actions = send(&mut session, asdu(), at(base, 3));
    assert_eq!(
        actions,
        vec![Action::Rejected(Rejection::TransferStopped(asdu()))]
    );
}

#[test]
fn an_i_frame_while_the_stop_is_pending_closes_the_connection() {
    let base = Instant::now();
    let mut session = started(base);
    send(&mut session, asdu(), at(base, 1));
    receive(
        &mut session,
        unnumbered(UnnumberedFunction::StopDtAct),
        at(base, 2),
    );
    let actions = receive(&mut session, information(0, 1), at(base, 3));
    assert_eq!(actions, vec![Action::Close(CloseReason::UnexpectedIFrame)]);
}

#[test]
fn normal_exchange_delivers_each_frame_and_acknowledges_every_w_frames() {
    let base = Instant::now();
    let mut session = started(base);
    for n in 0..8u16 {
        let actions = receive(&mut session, information(n, 0), at(base, u64::from(n)));
        if n == 7 {
            // w = 8: the acknowledgement is due after the eighth frame.
            assert_eq!(actions, vec![Action::Deliver(asdu()), send_s(8)]);
        } else {
            assert_eq!(actions, vec![Action::Deliver(asdu())], "frame {n}");
        }
    }
    assert_eq!(session.outstanding(), 0);
}

#[test]
fn sent_frames_carry_the_receive_number_and_follow_each_other() {
    let base = Instant::now();
    let mut session = started(base);
    receive(&mut session, information(0, 0), at(base, 1));
    receive(&mut session, information(1, 0), at(base, 1));
    // N(R) = 2 is piggybacked on the first frame sent.
    assert_eq!(
        send(&mut session, asdu(), at(base, 2)),
        vec![send_frame(0, 2)]
    );
    assert_eq!(
        send(&mut session, asdu(), at(base, 2)),
        vec![send_frame(1, 2)]
    );
    assert_eq!(session.outstanding(), 2);
}

#[test]
fn the_window_of_k_frames_is_full_until_the_peer_acknowledges() {
    let base = Instant::now();
    let mut session = started(base);
    // k = 12 by default.
    for n in 0..12u16 {
        assert_eq!(
            send(&mut session, asdu(), at(base, 1)),
            vec![send_frame(n, 0)]
        );
    }
    assert_eq!(session.outstanding(), 12);
    let refused = send(&mut session, asdu(), at(base, 1));
    assert_eq!(
        refused,
        vec![Action::Rejected(Rejection::WindowFull(asdu()))]
    );
    assert_eq!(session.outstanding(), 12, "a refused ASDU is not numbered");

    // The peer acknowledges four frames: the window opens again.
    assert!(receive(&mut session, supervisory(4), at(base, 2)).is_empty());
    assert_eq!(session.outstanding(), 8);
    assert_eq!(
        send(&mut session, asdu(), at(base, 3)),
        vec![send_frame(12, 0)]
    );
}

#[test]
fn an_acknowledgement_of_frames_never_sent_closes_the_connection() {
    let base = Instant::now();
    let mut session = started(base);
    send(&mut session, asdu(), at(base, 1));
    send(&mut session, asdu(), at(base, 1));
    // Two frames sent: N(R) 3 acknowledges a frame that was never sent.
    let actions = receive(&mut session, supervisory(3), at(base, 2));
    assert_eq!(
        actions,
        vec![Action::Close(CloseReason::InvalidReceiveSequence {
            receive: sn(3)
        })]
    );
    assert!(session.is_closed());
}

#[test]
fn an_acknowledgement_up_to_the_next_number_to_send_is_valid() {
    let base = Instant::now();
    let mut session = started(base);
    send(&mut session, asdu(), at(base, 1));
    send(&mut session, asdu(), at(base, 1));
    // N(R) 2 acknowledges everything sent: the boundary of the valid range.
    assert!(receive(&mut session, supervisory(2), at(base, 2)).is_empty());
    assert_eq!(session.outstanding(), 0);
    assert!(!session.is_closed());
}

#[test]
fn an_i_frame_with_an_unexpected_send_number_closes_the_connection() {
    let base = Instant::now();
    let mut session = started(base);
    // The next expected N(S) is 0; the frame carries 1.
    let actions = receive(&mut session, information(1, 0), at(base, 1));
    assert_eq!(
        actions,
        vec![Action::Close(CloseReason::SequenceError {
            expected: sn(0),
            received: sn(1),
        })]
    );
}

#[test]
fn an_i_frame_with_an_invalid_receive_number_closes_the_connection() {
    let base = Instant::now();
    let mut session = started(base);
    let actions = receive(&mut session, information(0, 5), at(base, 1));
    assert_eq!(
        actions,
        vec![Action::Close(CloseReason::InvalidReceiveSequence {
            receive: sn(5)
        })]
    );
}

#[test]
fn wrap_around_of_the_received_numbers_at_32768() {
    let base = Instant::now();
    let mut session = started(base);
    // Frames 0 to 32771: the N(S) wraps from 32767 to 0 and carries on.
    for n in 0..32772u32 {
        let number = (n % 32768) as u16;
        let actions = receive(&mut session, information(number, 0), at(base, 1));
        let receive_state = (n + 1) % 32768;
        let expected = if (n + 1) % 8 == 0 {
            vec![Action::Deliver(asdu()), send_s(receive_state as u16)]
        } else {
            vec![Action::Deliver(asdu())]
        };
        assert_eq!(actions, expected, "frame {n}");
    }
    assert!(!session.is_closed());
}

#[test]
fn wrap_around_of_the_sent_numbers_at_32768() {
    let base = Instant::now();
    let mut session = started(base);
    // Each frame is acknowledged before the next is sent, so the window never fills.
    for n in 0..32772u32 {
        let number = (n % 32768) as u16;
        let actions = send(&mut session, asdu(), at(base, 1));
        assert_eq!(actions, vec![send_frame(number, 0)], "frame {n}");
        let acknowledged = ((n + 1) % 32768) as u16;
        assert!(receive(&mut session, supervisory(acknowledged), at(base, 1)).is_empty());
        assert_eq!(session.outstanding(), 0);
    }
    assert!(!session.is_closed());
}

#[test]
fn a_window_that_crosses_the_wrap_is_full_after_k_frames() {
    let base = Instant::now();
    let mut session = started(base);
    // Acknowledge up to 32762, then send 12 frames across the wrap.
    for n in 0..32762u32 {
        send(&mut session, asdu(), at(base, 1));
        receive(
            &mut session,
            supervisory(((n + 1) % 32768) as u16),
            at(base, 1),
        );
    }
    for n in 32762..32774u32 {
        let number = (n % 32768) as u16;
        assert_eq!(
            send(&mut session, asdu(), at(base, 2)),
            vec![send_frame(number, 0)]
        );
    }
    assert_eq!(session.outstanding(), 12);
    assert_eq!(
        send(&mut session, asdu(), at(base, 2)),
        vec![Action::Rejected(Rejection::WindowFull(asdu()))]
    );
}

#[test]
fn a_frame_split_at_every_byte_boundary_gives_the_same_actions() {
    let base = Instant::now();
    let frame = information(0, 0);
    for split in 0..=frame.len() {
        let mut session = started(base);
        let (head, tail) = frame.split_at(split);
        let mut actions = receive(&mut session, head.to_vec(), at(base, 1));
        actions.extend(receive(&mut session, tail.to_vec(), at(base, 1)));
        assert_eq!(actions, vec![Action::Deliver(asdu())], "split at {split}");
    }
}

#[test]
fn several_frames_in_one_chunk_are_all_handled_in_order() {
    let base = Instant::now();
    let mut session = started(base);
    let chunk = [
        information(0, 0),
        information(1, 0),
        unnumbered(UnnumberedFunction::TestFrAct),
    ]
    .concat();
    let actions = receive(&mut session, chunk, at(base, 1));
    assert_eq!(
        actions,
        vec![
            Action::Deliver(asdu()),
            Action::Deliver(asdu()),
            send_u(UnnumberedFunction::TestFrCon),
        ]
    );
}

#[test]
fn a_frame_that_cannot_be_decoded_closes_the_connection() {
    let base = Instant::now();
    let mut session = started(base);
    // A U frame without a function.
    let actions = receive(
        &mut session,
        vec![0x68, 0x04, 0x03, 0x00, 0x00, 0x00],
        at(base, 1),
    );
    assert_eq!(
        actions,
        vec![Action::Close(CloseReason::Decode(
            DecodeError::InvalidControlField {
                control: [0x03, 0x00, 0x00, 0x00],
            }
        ))]
    );
}

#[test]
fn a_bad_start_octet_closes_the_connection_with_a_framing_error() {
    let base = Instant::now();
    let mut session = controlled(base);
    let actions = receive(
        &mut session,
        vec![0x69, 0x04, 0x07, 0x00, 0x00, 0x00],
        at(base, 1),
    );
    assert_eq!(
        actions,
        vec![Action::Close(CloseReason::Decode(
            DecodeError::FrameStart { found: 0x69 }
        ))]
    );
}

#[test]
fn a_closed_session_ignores_every_event() {
    let base = Instant::now();
    let mut session = controlled(base);
    receive(&mut session, information(0, 0), at(base, 1));
    assert!(session.is_closed());
    assert!(receive(
        &mut session,
        unnumbered(UnnumberedFunction::TestFrAct),
        at(base, 2)
    )
    .is_empty());
    assert!(send(&mut session, asdu(), at(base, 2)).is_empty());
    assert!(session.handle(Event::Tick, at(base, 3)).is_empty());
}

#[test]
fn an_asdu_sent_while_stopped_is_returned_to_the_user() {
    let base = Instant::now();
    let mut session = controlled(base);
    let actions = send(&mut session, asdu(), at(base, 1));
    assert_eq!(
        actions,
        vec![Action::Rejected(Rejection::TransferStopped(asdu()))]
    );
    assert_eq!(session.outstanding(), 0);
}

#[test]
fn start_and_stop_requests_are_rejected_on_the_controlled_station() {
    let base = Instant::now();
    let mut session = controlled(base);
    assert_eq!(
        session.handle(Event::StartDt, at(base, 1)),
        vec![Action::Rejected(Rejection::NotForRole(Request::StartDt))]
    );
    assert_eq!(
        session.handle(Event::StopDt, at(base, 1)),
        vec![Action::Rejected(Rejection::NotForRole(Request::StopDt))]
    );
}

/// A controlling session whose peer has confirmed STARTDT (figure 18).
fn controlling_started(base: Instant) -> Session {
    let mut session = Session::new(LinkConfig::with_defaults(Role::Controlling), base);
    assert_eq!(
        session.handle(Event::StartDt, at(base, 0)),
        vec![send_u(UnnumberedFunction::StartDtAct)]
    );
    assert!(receive(
        &mut session,
        unnumbered(UnnumberedFunction::StartDtCon),
        at(base, 0)
    )
    .is_empty());
    assert_eq!(session.transfer(), TransferState::Started);
    session
}

#[test]
fn controlling_start_sends_startdt_act_and_waits_for_the_confirmation() {
    let base = Instant::now();
    let mut session = Session::new(LinkConfig::with_defaults(Role::Controlling), base);
    assert_eq!(
        session.handle(Event::StartDt, at(base, 1)),
        vec![send_u(UnnumberedFunction::StartDtAct)]
    );
    assert_eq!(session.transfer(), TransferState::PendingStarted);
    assert!(receive(
        &mut session,
        unnumbered(UnnumberedFunction::StartDtCon),
        at(base, 2)
    )
    .is_empty());
    assert_eq!(session.transfer(), TransferState::Started);
}

#[test]
fn controlling_start_is_refused_unless_stopped() {
    let base = Instant::now();
    let mut session = controlling_started(base);
    assert_eq!(
        session.handle(Event::StartDt, at(base, 1)),
        vec![Action::Rejected(Rejection::InvalidState {
            request: Request::StartDt,
            state: TransferState::Started,
        })]
    );
}

#[test]
fn an_i_or_s_frame_while_the_start_is_pending_closes_the_connection() {
    let base = Instant::now();
    let mut session = Session::new(LinkConfig::with_defaults(Role::Controlling), base);
    session.handle(Event::StartDt, at(base, 0));
    assert_eq!(
        receive(&mut session, information(0, 0), at(base, 1)),
        vec![Action::Close(CloseReason::UnexpectedIFrame)]
    );
    let mut session = Session::new(LinkConfig::with_defaults(Role::Controlling), base);
    session.handle(Event::StartDt, at(base, 0));
    assert_eq!(
        receive(&mut session, supervisory(0), at(base, 1)),
        vec![Action::Close(CloseReason::UnexpectedSFrame)]
    );
}

#[test]
fn controlling_stop_without_unconfirmed_frames_waits_for_stopdt_con() {
    let base = Instant::now();
    let mut session = controlling_started(base);
    assert_eq!(
        session.handle(Event::StopDt, at(base, 1)),
        vec![send_u(UnnumberedFunction::StopDtAct)]
    );
    assert_eq!(session.transfer(), TransferState::PendingStopped);
    assert!(receive(
        &mut session,
        unnumbered(UnnumberedFunction::StopDtCon),
        at(base, 2)
    )
    .is_empty());
    assert_eq!(session.transfer(), TransferState::Stopped);
}

#[test]
fn controlling_stop_with_unconfirmed_sent_frames_waits_for_their_acknowledgement() {
    let base = Instant::now();
    let mut session = controlling_started(base);
    send(&mut session, asdu(), at(base, 1));
    assert_eq!(
        session.handle(Event::StopDt, at(base, 2)),
        vec![send_u(UnnumberedFunction::StopDtAct)]
    );
    assert_eq!(session.transfer(), TransferState::PendingUnconfirmedStop);
    // The controlled station acknowledges the frame: the stop is awaited.
    assert!(receive(&mut session, supervisory(1), at(base, 3)).is_empty());
    assert_eq!(session.transfer(), TransferState::PendingStopped);
    assert_eq!(
        receive(
            &mut session,
            unnumbered(UnnumberedFunction::StopDtCon),
            at(base, 4)
        ),
        vec![]
    );
    assert_eq!(session.transfer(), TransferState::Stopped);
}

#[test]
fn controlling_stop_confirms_the_received_frames_before_the_act() {
    let base = Instant::now();
    let mut session = controlling_started(base);
    assert_eq!(
        receive(&mut session, information(0, 0), at(base, 1)),
        vec![Action::Deliver(asdu())]
    );
    assert_eq!(
        session.handle(Event::StopDt, at(base, 2)),
        vec![send_s(1), send_u(UnnumberedFunction::StopDtAct)]
    );
    assert_eq!(session.transfer(), TransferState::PendingStopped);
}

#[test]
fn an_i_frame_while_pending_stopped_is_answered_at_once_and_the_state_stays() {
    // §5.3: the controlling station sends an S frame at once. Figure 18 shows
    // the loop without its action; the text is followed (QUESTIONS.md Q-008).
    let base = Instant::now();
    let mut session = controlling_started(base);
    session.handle(Event::StopDt, at(base, 1));
    assert_eq!(
        receive(&mut session, information(0, 0), at(base, 2)),
        vec![Action::Deliver(asdu()), send_s(1)]
    );
    assert_eq!(session.transfer(), TransferState::PendingStopped);
}

#[test]
fn an_s_frame_while_pending_stopped_keeps_waiting_for_the_confirmation() {
    let base = Instant::now();
    let mut session = controlling_started(base);
    session.handle(Event::StopDt, at(base, 1));
    assert!(receive(&mut session, supervisory(0), at(base, 2)).is_empty());
    assert_eq!(session.transfer(), TransferState::PendingStopped);
}

#[test]
fn an_i_frame_that_acknowledges_the_last_sent_frame_moves_to_pending_stopped() {
    let base = Instant::now();
    let mut session = controlling_started(base);
    send(&mut session, asdu(), at(base, 1));
    session.handle(Event::StopDt, at(base, 2));
    // The peer's I frame carries N(R) = 1: our only frame is acknowledged.
    assert_eq!(
        receive(&mut session, information(0, 1), at(base, 3)),
        vec![Action::Deliver(asdu()), send_s(1)]
    );
    assert_eq!(session.transfer(), TransferState::PendingStopped);
}

#[test]
fn asdus_are_not_sent_while_the_controlling_stop_is_pending() {
    let base = Instant::now();
    let mut session = controlling_started(base);
    session.handle(Event::StopDt, at(base, 1));
    assert_eq!(
        send(&mut session, asdu(), at(base, 2)),
        vec![Action::Rejected(Rejection::TransferStopped(asdu()))]
    );
}

#[test]
fn controlling_stop_is_refused_unless_started() {
    let base = Instant::now();
    let mut session = Session::new(LinkConfig::with_defaults(Role::Controlling), base);
    assert_eq!(
        session.handle(Event::StopDt, at(base, 1)),
        vec![Action::Rejected(Rejection::InvalidState {
            request: Request::StopDt,
            state: TransferState::Stopped,
        })]
    );
}

#[test]
fn a_stopdt_con_while_stopped_changes_nothing_on_the_controlling_station() {
    let base = Instant::now();
    let mut session = Session::new(LinkConfig::with_defaults(Role::Controlling), base);
    assert!(receive(
        &mut session,
        unnumbered(UnnumberedFunction::StopDtCon),
        at(base, 1)
    )
    .is_empty());
    assert_eq!(session.transfer(), TransferState::Stopped);
}

#[test]
fn the_time_of_the_last_frame_follows_the_clock() {
    let base = Instant::now();
    let mut session = controlled(base);
    receive(
        &mut session,
        unnumbered(UnnumberedFunction::TestFrAct),
        at(base, 5),
    );
    assert_eq!(session.last_received(), Some(at(base, 5)));
    receive(
        &mut session,
        unnumbered(UnnumberedFunction::TestFrAct),
        at(base, 9),
    );
    assert_eq!(session.last_received(), Some(at(base, 9)));
}

#[test]
fn the_window_parameter_is_the_one_of_the_configuration() {
    let base = Instant::now();
    let parameters = Parameters {
        k: 2,
        w: 1,
        ..Parameters::default()
    };
    let mut session = controlled_with(parameters, base);
    receive(
        &mut session,
        unnumbered(UnnumberedFunction::StartDtAct),
        at(base, 0),
    );
    // w = 1: every received frame is acknowledged at once.
    assert_eq!(
        receive(&mut session, information(0, 0), at(base, 1)),
        vec![Action::Deliver(asdu()), send_s(1)]
    );
    // k = 2: the third ASDU is refused.
    send(&mut session, asdu(), at(base, 2));
    send(&mut session, asdu(), at(base, 2));
    assert_eq!(
        send(&mut session, asdu(), at(base, 2)),
        vec![Action::Rejected(Rejection::WindowFull(asdu()))]
    );
}

mod timers;
