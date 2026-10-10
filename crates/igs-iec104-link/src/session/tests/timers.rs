// SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
//
// SPDX-License-Identifier: Apache-2.0

//! Timer tests with a fake clock: t1, t2 and t3, their reset rules, and the
//! deadline a caller waits for. The default parameters are t1 15 s, t2 10 s,
//! t3 20 s, k 12 and w 8.

use std::time::Duration;

use super::*;

#[test]
fn t1_closes_when_an_i_frame_is_not_confirmed_in_time() {
    let base = Instant::now();
    let mut session = started(base);
    send(&mut session, asdu(), at(base, 1));
    assert_eq!(session.next_deadline(), Some(at(base, 16)));
    assert!(session.handle(Event::Tick, at(base, 15)).is_empty());
    assert_eq!(
        session.handle(Event::Tick, at(base, 16)),
        vec![Action::Close(CloseReason::T1Expired)]
    );
    assert!(session.is_closed());
    assert_eq!(session.next_deadline(), None);
}

#[test]
fn t1_restarts_with_each_i_frame_sent() {
    let base = Instant::now();
    let mut session = started(base);
    send(&mut session, asdu(), at(base, 1));
    send(&mut session, asdu(), at(base, 10));
    // The last frame was sent at 10 s, so t1 is due at 25 s, not 16 s.
    assert!(session.handle(Event::Tick, at(base, 16)).is_empty());
    assert_eq!(
        session.handle(Event::Tick, at(base, 25)),
        vec![Action::Close(CloseReason::T1Expired)]
    );
}

#[test]
fn t1_stops_when_every_sent_frame_is_acknowledged() {
    let base = Instant::now();
    let mut session = started(base);
    send(&mut session, asdu(), at(base, 1));
    // The acknowledgement at 5 s also restarts t3, which is then due at 25 s.
    assert!(receive(&mut session, supervisory(1), at(base, 5)).is_empty());
    assert!(session.handle(Event::Tick, at(base, 16)).is_empty());
    assert!(session.handle(Event::Tick, at(base, 24)).is_empty());
    assert_eq!(
        session.handle(Event::Tick, at(base, 25)),
        vec![send_u(UnnumberedFunction::TestFrAct)]
    );
}

#[test]
fn a_partial_acknowledgement_keeps_t1_running_from_the_last_frame() {
    let base = Instant::now();
    let mut session = started(base);
    send(&mut session, asdu(), at(base, 1));
    send(&mut session, asdu(), at(base, 2));
    // The first frame is acknowledged; the last one, sent at 2 s, is not.
    assert!(receive(&mut session, supervisory(1), at(base, 5)).is_empty());
    assert_eq!(session.next_deadline(), Some(at(base, 17)));
    assert_eq!(
        session.handle(Event::Tick, at(base, 17)),
        vec![Action::Close(CloseReason::T1Expired)]
    );
}

#[test]
fn t1_expires_for_a_start_act_without_its_confirmation() {
    let base = Instant::now();
    let mut session = Session::new(LinkConfig::with_defaults(Role::Controlling), base);
    assert_eq!(
        session.handle(Event::StartDt, at(base, 0)),
        vec![send_u(UnnumberedFunction::StartDtAct)]
    );
    assert!(session.handle(Event::Tick, at(base, 14)).is_empty());
    assert_eq!(
        session.handle(Event::Tick, at(base, 15)),
        vec![Action::Close(CloseReason::T1Expired)]
    );
}

#[test]
fn t1_expires_for_a_stop_act_without_its_confirmation() {
    let base = Instant::now();
    let mut session = controlling_started(base);
    assert_eq!(
        session.handle(Event::StopDt, at(base, 1)),
        vec![send_u(UnnumberedFunction::StopDtAct)]
    );
    assert_eq!(session.transfer(), TransferState::PendingStopped);
    assert!(session.handle(Event::Tick, at(base, 15)).is_empty());
    assert_eq!(
        session.handle(Event::Tick, at(base, 16)),
        vec![Action::Close(CloseReason::T1Expired)]
    );
}

#[test]
fn t1_expires_for_a_test_act_without_its_confirmation() {
    let base = Instant::now();
    let mut session = started(base);
    assert!(session.handle(Event::Tick, at(base, 19)).is_empty());
    assert_eq!(
        session.handle(Event::Tick, at(base, 20)),
        vec![send_u(UnnumberedFunction::TestFrAct)]
    );
    assert!(session.handle(Event::Tick, at(base, 34)).is_empty());
    assert_eq!(
        session.handle(Event::Tick, at(base, 35)),
        vec![Action::Close(CloseReason::T1Expired)]
    );
}

#[test]
fn a_frame_arriving_after_t1_expired_is_not_handled() {
    let base = Instant::now();
    let mut session = started(base);
    send(&mut session, asdu(), at(base, 1));
    // The expiry comes first: the frame is not delivered.
    assert_eq!(
        receive(&mut session, information(0, 0), at(base, 17)),
        vec![Action::Close(CloseReason::T1Expired)]
    );
}

#[test]
fn a_test_confirmation_ends_the_wait_and_t3_counts_idle_time_again() {
    let base = Instant::now();
    let mut session = started(base);
    assert_eq!(
        session.handle(Event::Tick, at(base, 20)),
        vec![send_u(UnnumberedFunction::TestFrAct)]
    );
    // The confirmation at 25 s ends the wait for t1 and restarts t3, due at 45 s.
    assert!(receive(
        &mut session,
        unnumbered(UnnumberedFunction::TestFrCon),
        at(base, 25)
    )
    .is_empty());
    assert!(session.handle(Event::Tick, at(base, 35)).is_empty());
    assert!(session.handle(Event::Tick, at(base, 44)).is_empty());
    assert_eq!(
        session.handle(Event::Tick, at(base, 45)),
        vec![send_u(UnnumberedFunction::TestFrAct)]
    );
}

#[test]
fn t2_sends_the_acknowledgement_of_a_received_frame() {
    let base = Instant::now();
    let mut session = started(base);
    assert_eq!(
        receive(&mut session, information(0, 0), at(base, 1)),
        vec![Action::Deliver(asdu())]
    );
    // w = 8 is not reached, so the acknowledgement waits for t2: due at 11 s.
    assert_eq!(session.next_deadline(), Some(at(base, 11)));
    assert!(session.handle(Event::Tick, at(base, 10)).is_empty());
    assert_eq!(session.handle(Event::Tick, at(base, 11)), vec![send_s(1)]);
    // The frame received at 1 s restarted t3, which is due at 21 s.
    assert_eq!(session.next_deadline(), Some(at(base, 21)));
}

#[test]
fn t2_is_not_restarted_by_later_frames() {
    let base = Instant::now();
    let mut session = started(base);
    receive(&mut session, information(0, 0), at(base, 1));
    receive(&mut session, information(1, 0), at(base, 6));
    // t2 counts from the first unacknowledged frame, so it is still due at 11 s.
    assert_eq!(session.next_deadline(), Some(at(base, 11)));
    assert_eq!(session.handle(Event::Tick, at(base, 11)), vec![send_s(2)]);
}

#[test]
fn an_i_frame_sent_carries_the_acknowledgement_and_cancels_t2() {
    let base = Instant::now();
    let mut session = started(base);
    receive(&mut session, information(0, 0), at(base, 1));
    assert_eq!(
        send(&mut session, asdu(), at(base, 3)),
        vec![send_frame(0, 1)]
    );
    // The frame carried N(R) = 1, so no S frame is due at 11 s. t1 for the
    // frame sent is due at 18 s, which is the next deadline.
    assert_eq!(session.next_deadline(), Some(at(base, 18)));
    assert!(session.handle(Event::Tick, at(base, 11)).is_empty());
}

#[test]
fn an_idle_connection_is_tested_even_when_stopped() {
    let base = Instant::now();
    let mut session = controlled(base);
    assert!(session.handle(Event::Tick, at(base, 19)).is_empty());
    assert_eq!(
        session.handle(Event::Tick, at(base, 20)),
        vec![send_u(UnnumberedFunction::TestFrAct)]
    );
    assert_eq!(session.transfer(), TransferState::Stopped);
    // No confirmation: t1 closes the connection from the Stopped state too.
    assert_eq!(
        session.handle(Event::Tick, at(base, 35)),
        vec![Action::Close(CloseReason::T1Expired)]
    );
}

#[test]
fn a_received_frame_restarts_t3() {
    let base = Instant::now();
    let mut session = started(base);
    assert!(receive(&mut session, supervisory(0), at(base, 15)).is_empty());
    assert!(session.handle(Event::Tick, at(base, 20)).is_empty());
    assert_eq!(
        session.handle(Event::Tick, at(base, 35)),
        vec![send_u(UnnumberedFunction::TestFrAct)]
    );
}

#[test]
fn a_test_act_from_the_peer_restarts_t3_and_is_confirmed() {
    let base = Instant::now();
    let mut session = started(base);
    assert_eq!(
        receive(
            &mut session,
            unnumbered(UnnumberedFunction::TestFrAct),
            at(base, 15)
        ),
        vec![send_u(UnnumberedFunction::TestFrCon)]
    );
    assert!(session.handle(Event::Tick, at(base, 20)).is_empty());
    assert_eq!(
        session.handle(Event::Tick, at(base, 35)),
        vec![send_u(UnnumberedFunction::TestFrAct)]
    );
}

#[test]
fn a_second_test_waits_for_the_confirmation_of_the_first() {
    let base = Instant::now();
    let parameters = Parameters {
        t3: Duration::from_secs(5),
        ..Parameters::default()
    };
    let mut session = controlled_with(parameters, base);
    receive(
        &mut session,
        unnumbered(UnnumberedFunction::StartDtAct),
        at(base, 0),
    );
    // t3 (5 s) sends the first test; t3 expires again at 10 s and 15 s, but
    // the first test is still unconfirmed, so no second test is sent.
    assert_eq!(
        session.handle(Event::Tick, at(base, 5)),
        vec![send_u(UnnumberedFunction::TestFrAct)]
    );
    assert!(session.handle(Event::Tick, at(base, 10)).is_empty());
    assert!(session.handle(Event::Tick, at(base, 15)).is_empty());
    // t1 of the first test expires at 20 s.
    assert_eq!(
        session.handle(Event::Tick, at(base, 20)),
        vec![Action::Close(CloseReason::T1Expired)]
    );
}

#[test]
fn an_unsolicited_test_confirmation_changes_nothing_but_restarts_t3() {
    let base = Instant::now();
    let mut session = started(base);
    assert!(receive(
        &mut session,
        unnumbered(UnnumberedFunction::TestFrCon),
        at(base, 3)
    )
    .is_empty());
    assert_eq!(session.transfer(), TransferState::Started);
    assert!(!session.is_closed());
    assert_eq!(session.next_deadline(), Some(at(base, 23)));
}

#[test]
fn a_t3_too_long_for_the_clock_never_expires_and_never_panics() {
    let base = Instant::now();
    let parameters = Parameters {
        t3: Duration::from_secs(u64::MAX),
        ..Parameters::default()
    };
    let mut session = controlled_with(parameters, base);
    assert!(session.handle(Event::Tick, at(base, 1_000_000)).is_empty());
    assert!(!session.is_closed());
}
