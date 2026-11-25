# Campaigns on loopback (issue #3)

Two of the campaigns of issue #3 are run here: the general interrogation under load, and the switchover under
load. Both run in one process over TCP on 127.0.0.1: the controlled station (`igs_iec104::server::Server`) and the
controlling station (`igs_iec104::client::Client`). Each campaign is a program of `bench/campaign/`, and
`bench/campaign/run.sh` runs both and writes the reports of this directory.

| Campaign | Report | Program option | Result |
| --- | --- | --- | --- |
| General interrogation of 10 000 points, with spontaneous traffic on the same connection | [load.md](load.md) | `load` | PASS: every interrogation answered every point |
| Switchover under load, two stations in one redundancy group, the connection cut during the load | [switchover.md](switchover.md) | `switchover` | PASS: no change lost, in order, at most one resend per change |
| Interop with two independent peers (controlling and controlled roles) | not run | - | see below |

Reproduce, from the repository root:

```bash
bench/campaign/run.sh
```

The reports carry the machine, the commit and the command of their run. `LOAD_SECONDS` sets the length of the load
campaign (60 s by default). Each campaign exits with failure when its verdict is FAIL.

## What the campaigns check

- **Load.** The station has 10 000 float points in group 1. The first 1 000 change in turn, 1 000 changes a second in all.
  A general interrogation starts 250 ms after the previous one ended, on the connection that also carries the changes.
  The campaign checks that every interrogation returns the 10 000 information objects, and it reports the percentiles of
  the time of each interrogation (from the request to the termination) and of the time from a change to its reception.
  The changes are timed on the same clock: each change carries a sequence number, and the campaign keeps its instant.
- **Switchover.** Two stations (common addresses 1 and 2) share one listener, so one redundancy group. Each of their
  2 000 points changes in turn, 2 000 changes a second in all. The controlling station connects through a relay on
  loopback; ten seconds into the load the relay closes the connection, the client reconnects through it, starts the
  data transfer again, and receives the changes for ten seconds more. The changes then stop, and the connection receives
  what is left. The value of the n-th change of a point is n, so the campaign checks, per point:
  - no change is lost (every change the station made is received at the end);
  - the changes arrive in order (a first delivery never comes after a later change);
  - a change is delivered at most twice, once and once as a resend (§10.5, §10.6).

## Results

The reports are the output of the last run of `run.sh`. The load campaign runs for 60 s, with about 80 interrogations.
In the interrogations, the 10 000 points are answered in about 0.5 s. A change waits in the station's queue behind the
interrogation in progress, which is why the times from a change to its reception are about the length of an
interrogation: the queue is first in, first out, as the station is specified to send it.

## Defects found by the campaigns, and fixed

Both campaigns found defects in the station before they passed. Each fix has a test that fails without it.

1. **Commands lost when a connection ends.** The station hands its ASDUs to the transport through a channel. When the
   peer closed the connection, the ASDUs still queued in that channel were dropped, and no connection took them over. The
   station now takes them back with the unacknowledged ASDUs (`src/server.rs`, `serve`). Without the fix, the switchover
   campaign lost 10 changes in each of two runs of 3 s + 3 s; with it, none.
2. **An ASDU overtaking a refused one.** When an acknowledgement opened the window of I frames, an ASDU queued in the
   transport's channel behind a refused one was sent first. The refused one is sent again later by the station, so the
   peer received them out of order. In a general interrogation, the answer to the refused point arrived after the
   termination of the interrogation, and that point was missing from it: in one of two 60 s runs before the fix, three of
   80 interrogations lacked one point each. The transport now refuses the ASDUs queued behind a refused one too, and hands
   them back in order (`src/transport.rs`, `drive`). The test
   `an_asdu_queued_behind_one_refused_for_a_full_window_is_not_sent_ahead_of_it` (`tests/transport.rs`) fails without the fix
   in each of three runs tried, in one of its first 17 rounds.

## Not run here: the interop campaign

The interop campaign needs the reference peers of `bench/` (the Fledge image of the reference server and client, and the
OpenMUC j60870 library in a Java image), which run in Docker, and `tshark` to read the captures. This environment has
neither Docker, nor a Java runtime, nor `tshark`, so the interop campaign was not run. The scenarios exist in `bench/tests/`
(`test_t4_client_matrix.py`, `test_s5_interop_matrix.py`, `test_s5_load.py`, `test_s5_failover.py`), and they are the
ones to run where Docker is available. Their reports are not in this directory yet.

## Limits

- Both campaigns run the controlling and the controlled station in one process on loopback. They measure the
  implementation, not a network, and they do not replace the interop against independent peers.
- One machine, one run for each report. The percentiles of the general interrogation come from about 80 samples, so
  the 99.9th percentile is close to the maximum.
