# SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
#
# SPDX-License-Identifier: Apache-2.0
"""L4: igs-iec104 against the reference peers, in both directions.

The probe (bench/probe) runs one scenario per test, inside the bench network,
and its own tcpdump captures every frame it exchanges. Direction 1 (`d1-*`):
the probe is the controlling station and connects to the reference server.
Direction 2 (`d2-*`): the probe is the controlled station and the reference
client connects to it.

Values used as data: type IDs 1 (M_SP_NA_1) and 100 (C_IC_NA_1); causes 3, 6,
7 and 10 (spontaneous, activation, activation confirmation, termination). The
window and the acknowledgements are checked from N(S) and N(R) of the frames,
with k = 12 and w = 8, the values of both sides' configurations.
"""

from __future__ import annotations

import time

import pytest

from igs_bench import capture, probe, tshark
from igs_bench.compose import Bench

PROBE = "172.31.104.30"
SERVER = "172.31.104.10"
CLIENT = "172.31.104.21"
PROBE_BIN = probe.CONTAINER_BINARY
TO_PROBE = "/bench/peer/client-to-probe.json"

M_SP_NA_1, C_IC_NA_1 = 1, 100
SPONTANEOUS, ACT, ACTCON, ACTTERM = 3, 6, 7, 10
K, W, T2_SECONDS = 12, 8, 10
MODULUS = 32768

pytestmark = pytest.mark.usefixtures("probe_binary")


@pytest.fixture(scope="module")
def probe_binary():
    return probe.build()


@pytest.fixture
def bench_for(request):
    """Makes benches for one test; each is torn down at the end of the test."""
    benches: list[Bench] = []

    def make(env: dict[str, str] | None = None) -> Bench:
        capture.CAPTURE_DIR.mkdir(exist_ok=True)
        name = f"{request.node.name}-{time.strftime('%Y%m%dT%H%M%S')}.pcap"
        instance = Bench(name, env=env)
        instance.down()
        benches.append(instance)
        return instance

    yield make
    for instance in benches:
        instance.down()


def _run_probe(bench: Bench, scenario: str, address: str) -> None:
    result = bench.exec("probe", PROBE_BIN, scenario, address, timeout=300, check=False)
    assert result.returncode == 0, f"{scenario} failed:\n{result.stdout}{result.stderr}"
    assert f"PASS {scenario}" in result.stdout


def _probe_frames(bench: Bench) -> list[tshark.Frame]:
    frames = tshark.decode(bench, "probe")
    assert tshark.errors(bench, "probe") == []
    return frames


def _assert_window_respected(frames: list[tshark.Frame], source: str, peer: str) -> int:
    """Before each I frame of `source`, fewer than k of its frames are unacknowledged."""
    acknowledged = 0  # N(R) of the peer, as far as the capture shows: 0 at the start
    checked = 0
    for frame in frames:
        if frame.src == peer and frame.rx is not None:
            acknowledged = frame.rx
        if frame.src == source and frame.kind == "I":
            outstanding = (frame.tx - acknowledged) % MODULUS
            assert outstanding < K, (
                f"frame {frame.number}: {outstanding} frames unacknowledged before N(S)={frame.tx}"
            )
            checked += 1
    assert checked > 0
    return checked


def _first_ack(frames: list[tshark.Frame], peer: str, value: int, after: tshark.Frame) -> tshark.Frame | None:
    for frame in frames:
        if frame.number > after.number and frame.src == peer and frame.rx == value:
            return frame
    return None


def test_d1_startdt_interrogation_and_stopdt(bench_for):
    bench = bench_for()
    bench.up("ref-server", "probe")
    bench.wait_ready("ref-server")
    _run_probe(bench, "d1-startdt-gi-stopdt", f"{SERVER}:2404")
    capture.stop(bench, "probe")

    frames = _probe_frames(bench)
    assert tshark.find(frames, src=PROBE, info="STARTDT act")
    assert tshark.find(frames, src=SERVER, info="STARTDT con")
    assert tshark.find(frames, src=PROBE, typeid=C_IC_NA_1, cause=ACT)
    assert tshark.find(frames, src=SERVER, typeid=C_IC_NA_1, cause=ACTCON)
    assert tshark.find(frames, src=SERVER, typeid=C_IC_NA_1, cause=ACTTERM)
    assert tshark.find(frames, src=PROBE, info="STOPDT act")
    assert tshark.find(frames, src=SERVER, info="STOPDT con")


def test_d1_idle_connection_is_tested_and_stays_open(bench_for):
    bench = bench_for()
    bench.up("ref-server", "probe")
    bench.wait_ready("ref-server")
    _run_probe(bench, "d1-testfr-idle", f"{SERVER}:2404")
    capture.stop(bench, "probe")

    frames = _probe_frames(bench)
    assert tshark.find(frames, src=PROBE, info="TESTFR act")
    assert tshark.find(frames, src=SERVER, info="TESTFR con")


def test_d1_window_of_k_frames_is_respected(bench_for):
    bench = bench_for()
    bench.up("ref-server", "probe")
    bench.wait_ready("ref-server")
    _run_probe(bench, "d1-window", f"{SERVER}:2404")
    capture.stop(bench, "probe")

    frames = _probe_frames(bench)
    _assert_window_respected(frames, source=PROBE, peer=SERVER)


def test_d2_reference_client_starts_and_interrogates_the_probe(bench_for):
    bench = bench_for({"BENCH_CLIENT_CONFIG": TO_PROBE})
    bench.up("probe")
    proc = bench.exec_async("probe", PROBE_BIN, "d2-startdt-gi", "0.0.0.0:2404")
    try:
        first = proc.stdout.readline()
        assert first.startswith("listening"), first
        bench.up("ref-client")
        bench.wait_ready("ref-client")
        rest, _ = proc.communicate(timeout=300)
    finally:
        if proc.poll() is None:
            proc.kill()
    assert proc.returncode == 0, first + rest
    assert "PASS d2-startdt-gi" in first + rest
    capture.stop(bench, "probe")

    frames = _probe_frames(bench)
    assert tshark.find(frames, src=CLIENT, info="STARTDT act")
    assert tshark.find(frames, src=PROBE, info="STARTDT con")
    assert tshark.find(frames, src=CLIENT, typeid=C_IC_NA_1, cause=ACT)
    assert tshark.find(frames, src=PROBE, typeid=C_IC_NA_1, cause=ACTCON)
    assert tshark.find(frames, src=PROBE, typeid=C_IC_NA_1, cause=ACTTERM)


def test_d2_idle_probe_is_tested_and_stays_open(bench_for):
    bench = bench_for({"BENCH_CLIENT_CONFIG": TO_PROBE})
    bench.up("probe")
    proc = bench.exec_async("probe", PROBE_BIN, "d2-testfr-idle", "0.0.0.0:2404")
    try:
        first = proc.stdout.readline()
        assert first.startswith("listening"), first
        bench.up("ref-client")
        bench.wait_ready("ref-client")
        rest, _ = proc.communicate(timeout=300)
    finally:
        if proc.poll() is None:
            proc.kill()
    assert proc.returncode == 0, first + rest
    capture.stop(bench, "probe")

    frames = _probe_frames(bench)
    # The probe's own t3 sends the test act; the reference client confirms it. The
    # reference client sends none while the probe sends test frames (5.2).
    assert tshark.find(frames, src=PROBE, info="TESTFR act")
    assert tshark.find(frames, src=CLIENT, info="TESTFR con")


def test_d2_probe_frames_are_acknowledged_after_w_and_within_t2(bench_for):
    bench = bench_for({"BENCH_CLIENT_CONFIG": TO_PROBE})
    bench.up("probe")
    proc = bench.exec_async("probe", PROBE_BIN, "d2-ack-after-w", "0.0.0.0:2404")
    try:
        first = proc.stdout.readline()
        assert first.startswith("listening"), first
        bench.up("ref-client")
        bench.wait_ready("ref-client")
        rest, _ = proc.communicate(timeout=300)
    finally:
        if proc.poll() is None:
            proc.kill()
    assert proc.returncode == 0, first + rest
    capture.stop(bench, "probe")

    frames = _probe_frames(bench)
    sent = [f for f in frames if f.src == PROBE and f.kind == "I"]
    assert len(sent) == 11
    # w = 8: the eighth frame is acknowledged at once, the eleventh within t2.
    ack_eight = _first_ack(frames, CLIENT, W, sent[W - 1])
    assert ack_eight is not None, "the eight frames were not acknowledged"
    assert ack_eight.time - sent[W - 1].time < 2.0
    ack_eleven = _first_ack(frames, CLIENT, 11, sent[10])
    assert ack_eleven is not None, "the last three frames were not acknowledged"
    assert ack_eleven.time - sent[10].time <= T2_SECONDS + 2.0


def test_d1_peer_test_is_confirmed_and_the_connection_stays_open(bench_for):
    bench = bench_for()
    bench.up("ref-server", "probe")
    bench.wait_ready("ref-server")
    _run_probe(bench, "d1-testfr-from-peer", f"{SERVER}:2404")
    capture.stop(bench, "probe")

    frames = _probe_frames(bench)
    # The probe's t3 is long: any test act on the wire is the reference server's.
    assert not tshark.find(frames, src=PROBE, info="TESTFR act")
    assert tshark.find(frames, src=SERVER, info="TESTFR act")
    assert tshark.find(frames, src=PROBE, info="TESTFR con")


def test_d2_peer_test_is_confirmed_and_the_connection_stays_open(bench_for):
    bench = bench_for({"BENCH_CLIENT_CONFIG": TO_PROBE})
    bench.up("probe")
    proc = bench.exec_async("probe", PROBE_BIN, "d2-testfr-from-peer", "0.0.0.0:2404")
    try:
        first = proc.stdout.readline()
        assert first.startswith("listening"), first
        bench.up("ref-client")
        bench.wait_ready("ref-client")
        rest, _ = proc.communicate(timeout=300)
    finally:
        if proc.poll() is None:
            proc.kill()
    assert proc.returncode == 0, first + rest
    capture.stop(bench, "probe")

    frames = _probe_frames(bench)
    assert not tshark.find(frames, src=PROBE, info="TESTFR act")
    assert tshark.find(frames, src=CLIENT, info="TESTFR act")
    assert tshark.find(frames, src=PROBE, info="TESTFR con")


# Known gap: the reference client closes the connection about 60 s after it opens
# (FIN then RST, see docs/ai-log/L4.md). At the rate the window allows, the probe
# reaches about 10 800 frames in that time, short of the 32768 that wrap N(S).
@pytest.mark.xfail(
    strict=True,
    reason="the reference client closes the connection after about 60 s; N(S) wraps after 32768 frames",
)
def test_d2_sequence_numbers_wrap_at_32768_and_the_connection_stays_open(bench_for):
    bench = bench_for({"BENCH_CLIENT_CONFIG": TO_PROBE})
    bench.up("probe")
    proc = bench.exec_async("probe", PROBE_BIN, "d2-wrap", "0.0.0.0:2404")
    try:
        first = proc.stdout.readline()
        assert first.startswith("listening"), first
        bench.up("ref-client")
        bench.wait_ready("ref-client")
        rest, _ = proc.communicate(timeout=600)
    finally:
        if proc.poll() is None:
            proc.kill()
    assert proc.returncode == 0, first + rest
    assert "PASS d2-wrap" in first + rest
    capture.stop(bench, "probe")

    frames = _probe_frames(bench)
    sent = [f.tx for f in frames if f.src == PROBE and f.kind == "I"]
    assert len(sent) > MODULUS, "fewer frames than the modulus: the sequence cannot wrap"
    # Every accepted frame follows the one before it, modulo 32768: N(S) wraps 32767 -> 0.
    for before, after in zip(sent, sent[1:]):
        assert after == (before + 1) % MODULUS, f"N(S) {before} is followed by {after}"
    assert 0 in sent[1:] and MODULUS - 1 in sent
    # The peer acknowledged across the wrap, so its N(R) wraps too.
    acks = [f.rx for f in frames if f.src == CLIENT and f.rx is not None]
    assert any(before > MODULUS - 1000 and after < 1000 for before, after in zip(acks, acks[1:])), (
        "the peer's N(R) never wrapped"
    )
