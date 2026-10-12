# SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
#
# SPDX-License-Identifier: Apache-2.0
"""S5: the igs104-server station against the second reference peer (j60870), one procedure per row.

The station is the igs104-server binary, and the peer is the j60870 driver (bench/peer/j60870). The peer
is a controlling station, so the rows are the procedures a controlling station starts: the interrogations,
the counter interrogation, the read, the clock synchronization, the test and reset commands, and the
commands. Each row passes when the outcome the peer reports is the one the matrix expects.

Values used as data: type identifications and causes of IEC 60870-5-104 (for example cause 47, unknown
information object address, for a command to a point that takes none). The expected outcomes are the
station's behaviour as documented in docs/igs104-server.md of the upstream repository.

This test does not measure latency and does not test the switchover: S5 covers those in later rows.
"""

from __future__ import annotations

import time

import pytest

from igs_bench import s5
from igs_bench.compose import Bench

PEER = s5.PEER_SERVICE
SERVER = s5.SERVER_SERVICE

# procedure, expected kind, a text the outcome detail must contain (or None)
MATRIX = [
    ("interrogation", "passed", "terminated"),
    ("counter", "passed", "terminated"),
    ("read", "passed", None),
    ("clock", "passed", "confirmed"),
    ("test", "passed", "confirmed"),
    # Known gap (docs/ai-log/S5.md): the station does not implement the reset process command, and
    # it closes the connection on a test command without a time tag (C_TS_NA_1, outside the 104 profile).
    ("reset", "refused", "cause 7"),
    ("test-without-time-tag", "timeout", "no answer"),
    ("single", "passed", "terminated"),
    ("double", "passed", "terminated"),
    ("normalized", "passed", "terminated"),
    ("scaled", "passed", "terminated"),
    ("float", "passed", "terminated"),
    ("single-without-command", "refused", "cause 47"),
]


def _wait_listening(bench: Bench, timeout: float = 60) -> None:
    """Waits until the station listens on port 2404 (hexadecimal 0964 in /proc/net/tcp)."""
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        probe = bench.exec(SERVER, "grep", "-q", ":0964 .* 0A ", "/proc/net/tcp", check=False)
        if probe.returncode == 0:
            return
        time.sleep(1)
    logs = bench.run("logs", SERVER, check=False)
    raise TimeoutError(f"the station is not listening\n{logs.stdout}{logs.stderr}")


@pytest.fixture(scope="module")
def station():
    s5.build_server()
    bench = Bench("s5-interop-matrix.pcap", profiles=("s5",))
    bench.down()
    bench.up(SERVER, PEER)
    try:
        _wait_listening(bench)
        yield bench
    finally:
        bench.down()


@pytest.mark.parametrize(("procedure", "kind", "text"), MATRIX, ids=[row[0] for row in MATRIX])
def test_the_station_answers_the_peer(station, procedure, kind, text):
    result = station.exec(PEER, *s5.driver_command(procedure), timeout=180, check=False)
    assert result.returncode == 0, f"the driver failed:\n{result.stdout}{result.stderr}"
    outcome = s5.parse_outcome(result.stdout)
    assert outcome.kind == kind, f"{procedure}: {outcome}\n{result.stdout}"
    if text is not None:
        assert text in outcome.detail, f"{procedure}: {outcome}"
