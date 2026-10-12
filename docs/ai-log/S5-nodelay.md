# S5 — Turn off Nagle's algorithm on the connections

## Prompt (summarised)

The load test of the station under test (S5, part 2) measured a general interrogation of 10 010 points at
63 s, against a proposed threshold of 5 s. The maintainer asked for the fix of TCP_NODELAY as a separate task.

## Cause

The station and the client did not set TCP_NODELAY on their connections. The protocol's frames are short
and each is acknowledged by the peer before the next one is due, so Nagle's algorithm holds a frame back
until the acknowledgement of the frame before it. The peer acknowledges with a delay of its own, so the
frames left in batches about 40 ms apart. The capture of one session showed the batches: the peer's
acknowledgement of eight frames, then a gap of about 40 ms before the next frames.

## Change

- `transport::without_delay(stream)` sets TCP_NODELAY on a stream.
- `transport::connect` (the client, plain and secured) sets it on the connection it opens; a failure is
  reported as a connection error, as for any other connect failure.
- The station sets it on each accepted connection, before the connection is served. A failure does not
  refuse the connection: the connection is served with the delay of the algorithm, as before.
- A unit test checks the option on both sides of a loopback connection.

## Measurements (the S5 bench, 10 010 points, the load client of `bench/peer/j60870`)

| Session | Before | After |
| --- | --- | --- |
| first session of a fresh station | 62.7 s, 63.0 s | 15.8 s (four runs: 15.7 to 15.9 s) |
| second session on the same station | (no termination) | 38.6 s |

The 15.8 s that remain come from the load itself: the load points change 1 000 times per second, and the
changes share the link with the general interrogation. The threshold of 5 s is not met yet; the
measurement design is the next decision (see `docs/ai-log/S5.md`).

One run of the first build printed 10 009 objects instead of 10 010. The client now prints the addresses
it did not receive. Seven later runs of the fixed station answered all 10 010 objects; the one miss is not
explained, and the client will show it if it recurs.

## Verification

- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`: clean.
- `cargo test --workspace --all-features`: all tests pass, including the new one.
- `cargo deny check`: bans, licences and sources pass. Advisories fail on `unsafe-libyaml`
  (RUSTSEC-2024-0449), reached through `serde_yaml` of `xtask`. This failure is not new: the change does not
  touch `Cargo.lock`. It is for the maintainer to decide (replace the YAML parser of `xtask`, or accept the
  advisory for a development tool that is not published).

## Not in this change

- The load test and its design (a single long session, not the sequential sessions of the first draft).
- The backlog of the changes that wait for a connection while the station is stopped: the session that
  follows a stop receives them first (`docs/igs104-server.md`). Its effect on the interrogation time is
  measured, not changed.
- The station does not stop within 10 s on SIGTERM while a connection is open (seen under Podman). Not
  investigated here.
