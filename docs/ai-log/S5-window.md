# S5 — Send the held ASDUs when an acknowledgement opens the window

## Prompt (summarised)

After the change of TCP_NODELAY (`docs/ai-log/S5-nodelay.md`), a general interrogation of 10 010 points still
took about 15 s on the bench, with the load of 1 000 changing points. The maintainer asked for the correction
of the wake-up of the station's send loop.

## Cause

A station session holds the ASDUs it cannot send because the window of unacknowledged I frames is full (k = 12).
Such an ASDU is refused by the transport and kept by the session loop, which retries on a 10 ms timer
(`RETRY`). An acknowledgement that opens the window does not wake that loop: the held ASDUs wait for the next
tick. With the peer acknowledging every eight frames, each window of frames costs up to one tick, so 10 010
objects cost about 1 250 ticks: about 12 to 15 s.

An experiment with a 1 ms retry timer (not committed) brought one interrogation from 15.8 s to 3.1 s. The fix
below removes the timer from the path: an acknowledgement wakes the loop itself.

## Change

- `transport::Delivery::Window`: reported by a controlled session when an acknowledgement lowers the number of
  unacknowledged I frames. The transport compares the count before and after each event; a controlling
  connection does not report it, so a client sees no new event.
- The station's session loop takes the delivery and flushes the held ASDUs after it, like any other delivery.
- The retry timer stays: it still paces a refusal that the window keeps refusing.
- The client binary prints nothing for the delivery (it is not reached on a controlling connection).

## Test

`server::tests::an_acknowledgement_that_opens_the_window_sends_the_held_events_at_once`: thirty events against a
window of twelve, received in order, with the virtual time spent waiting below one retry tick. It failed before
the change ("the held events waited 30 ms for the retry timer") and passes after it.

## Measurements (the S5 bench)

| Case | Before (after the TCP_NODELAY change) | After |
| --- | --- | --- |
| fresh station, 1 000 changing points, one interrogation | 15.8 s | 0.71 s (10 010 objects) |

Rate sweep, after the change (one session of 120 s, an interrogation every 15 s, a command every 500 ms;
`bench/tests/test_s5_load.py`):

| Changes per second | Interrogations (p99) | Changes (p99) | Commands confirmed (p99) | Result |
| --- | --- | --- | --- | --- |
| 50 | 8, 654 ms | 447 ms | 230, 0 ms | passed |
| 100 | 8, 637 ms | 552 ms | 230, 0 ms | failed: changes above 500 ms |
| 200 | 8, 637 ms | 543 ms | 230, 0 ms | failed: changes above 500 ms |
| 500 | 8, 642 ms | 546 ms | 229, 0 ms | failed: changes above 500 ms |
| 1 000 | 8, 641 ms | 478 ms | 229, 0 ms | passed |

Before the change, every rate failed with a queue that grew to minutes (`docs/ai-log/S5-nodelay.md`). The
interrogations and the commands now meet their thresholds at every rate. The 99th percentile of the changes
stays between 450 and 550 ms whatever the rate, so it is no longer a queue: its cause is not yet identified.
The results at 100 to 500 changes per second sit on the 500 ms threshold, and are not monotonic with the rate.

## Verification

- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`: clean.
- `cargo test --workspace --all-features`: all tests pass, including the new one.
- `cargo deny check`: passes (the advisory is ignored, see `deny.toml`).

## Public API

`Delivery` gains the variant `Window`. A `match` on `Delivery` without a wildcard arm must add one; the crate's
own code and examples are updated. The change is recorded in the CHANGELOG.

## Investigation of the 500 ms threshold for the changes

Three measurements, one session each (`bench/peer/j60870/Load.java`, mode `session`):

1. **The backlog before the connection.** The first second of a session carries the changes raised while
   the station had no connection (`docs/igs104-server.md`: they wait for the next one). At 50 changes per
   second, these reached 17 s of latency. From the first second on, the latency was 3 to 7 ms, interrogations
   included. This part of the 99th percentile is a measurement artifact of the start, not a property of the
   steady state.
2. **The reading thread of the peer.** The first version printed one line per change inside the reading
   thread. Printing them at the end of the session moved the steady-state 99th percentile at 500 changes per
   second from 343 ms to 259 ms: a part of the tail came from the peer, but not most of it.
3. **The interrogation.** At 500 changes per second, the steady-state changes over 100 ms were exactly 500 of
   14 500 (the 99th percentile is 259 ms, the maximum 267 ms). They all arrived in the same second, at the
   interrogation of the 15 s cycle. The load points all change in the same tick (they share one period and
   start together), so each second brings a burst of 500 changes. A change raised while an interrogation is
   being answered waits behind the answers: the station's queue is first in, first out, with no priority for
   spontaneous changes.

So the 500 ms threshold is exceeded by the design of the load, which makes every burst coincide with the
interrogations, and by the first-in, first-out queue, which makes the changes wait for the answers. Neither is
a failure of a single procedure. Decisions for the maintainer: give spontaneous changes priority over the answers
of an interrogation (a change in the station's queue), or accept the interrogation's duration as the bound of a
change's wait under load; and decide whether a station may keep changes indefinitely while no client is
connected. The load test can also spread the change phases so that the bursts do not all coincide.

## Revised threshold for the changes, and the rate sweep

The maintainer accepted that the duration of an interrogation bounds the wait of a spontaneous change under
load: the station's queue is first in, first out, and an interrogation is answered before the changes behind
it. The load test now checks the 99th percentile of the changes against the longest interrogation of the same
session, and the load points have staggered periods so that their changes do not arrive in one burst.

Sweep after the change (one session of 120 s, an interrogation every 15 s, a command every 500 ms; the nominal
rate is the number of changing points, the measured rate is the number of changes received over the session):

| Nominal | Measured | Interrogations (p99) | Changes (p99) | Commands (p99) | Result |
| --- | --- | --- | --- | --- | --- |
| 50 | 49 per second | 636 ms | 475 ms | 0 ms | passed |
| 100 | 96 per second | 631 ms | 426 ms | 0 ms | passed |
| 200 | 183 per second | 630 ms | 421 ms | 0 ms | passed |
| 500 | 407 per second | 628 ms | 426 ms | 1 ms | passed |
| 1 000 | 697 per second | 642 ms | 423 ms | 1 ms | passed |

The changes stay below the longest interrogation, and every command is confirmed. The load test does not yet
cover the switchover under load, which remains for S5.
