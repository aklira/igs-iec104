# SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
#
# SPDX-License-Identifier: Apache-2.0
"""S5: the load test of igs104-server with 10 000 points, measured by the j60870 load client.

One connection runs for SESSION_SECONDS. While the station's changing points send their changes, the client
interrogates the station every INTERROGATE_EVERY seconds, and sends a command every 500 ms in between. The
changing points have staggered periods, so that their changes do not arrive in one burst per second.
The thresholds, for the maintainer to confirm (docs/ai-log/S5.md and docs/ai-log/S5-window.md):
- a station interrogation under load: the 99th percentile of the time to the termination is at most 5 s,
  and each one answers the 10 010 information objects of the station;
- a spontaneous change: the 99th percentile of the time from the change to its reception is at most the
  longest interrogation of the same session. The station's queue is first in, first out, and an interrogation
  is answered before the changes behind it: a change waits at most for the interrogation in progress;
- a command: the 99th percentile of the time from the execution to its confirmation is at most 200 ms, and
  no command fails.

A station starts one connection at a time (docs/igs104-server.md), so the load is one session, not
50 connections at once.
"""

from __future__ import annotations

import pytest

from igs_bench import load, s5
from igs_bench.compose import Bench

PEER = s5.PEER_SERVICE
SERVER = "load-server"
SESSION_SECONDS = 120
# Changes per second of the station (one change per point per second): a sweep from light to heavy load.
RATES = [50, 100, 200, 500, 1000]
INTERROGATE_EVERY = 15
GI_P99_MS = 5_000
COMMAND_P99_MS = 200
# The changes wait at most for an interrogation in progress: their bound is the longest of them.


@pytest.fixture(scope="module", params=RATES, ids=lambda rate: f"{rate}-changes-per-second")
def station(request):
    s5.build_server()
    expected = load.write_points(changing=request.param, stagger_ms=1)
    bench = Bench("s5-load.pcap", profiles=("s5", "load"))
    bench.down()
    bench.up(SERVER, PEER)
    try:
        s5.wait_listening(bench, SERVER)
        yield bench, expected
    finally:
        bench.down()


def test_the_station_under_load_answers_within_the_thresholds(station, request):
    bench, expected = station
    print(f"\nrate: {request.node.callspec.id}")
    command = [
        "java",
        "-cp",
        s5.PEER_CLASSPATH,
        "Load",
        "--host",
        s5.CONTAINER_LOAD_SERVER,
        "--port",
        "2404",
        "--common-address",
        s5.COMMON_ADDRESS,
        "--mode",
        "session",
        "--seconds",
        str(SESSION_SECONDS),
        "--every",
        str(INTERROGATE_EVERY),
        "--commands",
        "10",
    ]
    result = bench.exec(PEER, *command, timeout=SESSION_SECONDS + 600, check=False)
    output = result.stdout
    assert result.returncode == 0, f"the load client failed:\n{output}{result.stderr}"
    assert "session done" in output, output

    failed = [line for line in output.splitlines() if " failed" in line or "refused" in line]
    assert not failed, "\n".join(failed)

    counts = [
        int(dict(part.split("=", 1) for part in line.split() if "=" in part)["points"])
        for line in load.parse_lines(output, "gi load ")
    ]
    assert len(counts) >= SESSION_SECONDS // INTERROGATE_EVERY, f"only {len(counts)} interrogations"
    assert all(count == expected for count in counts), f"points per interrogation: {sorted(set(counts))}"

    interrogations = load.ms_values(output, "gi load ")
    changes = load.ms_values(output, "spont ms=")
    commands = load.ms_values(output, "cmd ms=")
    assert changes, "no spontaneous change was received"
    assert commands, "no command was confirmed"

    gi_p99 = load.percentile(interrogations, 0.99)
    change_p99 = load.percentile(changes, 0.99)
    command_p99 = load.percentile(commands, 0.99)
    change_bound = max(interrogations)
    print(
        f"interrogations: {len(interrogations)}, p99 {gi_p99:.0f} ms; "
        f"changes: {len(changes)}, p99 {change_p99:.0f} ms; "
        f"commands: {len(commands)}, p99 {command_p99:.0f} ms"
    )
    assert gi_p99 <= GI_P99_MS, f"interrogation p99 {gi_p99:.0f} ms is above {GI_P99_MS} ms"
    assert change_p99 <= change_bound, (
        f"change p99 {change_p99:.0f} ms is above the longest interrogation, {change_bound:.0f} ms"
    )
    assert command_p99 <= COMMAND_P99_MS, f"command p99 {command_p99:.0f} ms is above {COMMAND_P99_MS} ms"
