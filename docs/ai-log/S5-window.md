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
