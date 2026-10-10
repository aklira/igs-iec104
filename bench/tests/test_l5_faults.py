# SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
#
# SPDX-License-Identifier: Apache-2.0
"""L5: fault injection between igs-iec104 and the reference peers, both directions.

The probe runs each scenario inside the bench network, with a relay between its
driver and the peer (bench/probe/src/relay.rs). The relay drops, delays or
duplicates the frames a scenario names, so the peer sees the faulty stream and
the probe's capture shows what crossed the wire.

Direction 1 (`d1-*`): the probe is the controlling station, the relay in front of
the reference server. Direction 2 (`d2-*`): the reference client connects to the
relay, and the probe is the controlled station behind it.

Expectations come from 104 §5.1 (figure 11: a receiver of a disturbed sequence
closes the connection), §5.2 and §9.6 (t1 closes a connection whose frames or
acts are not confirmed in time), and figure 12 (t1 for a last I frame that is
not acknowledged). Figure 12 is read with its open question Q-009.
"""

from __future__ import annotations

import pytest

from igs_bench import capture, tshark
from igs_bench.compose import Bench

PROBE = "172.31.104.30"
SERVER = "172.31.104.10"
CLIENT = "172.31.104.21"
PROBE_BIN = "/bench/bin/igs-iec104-probe"
TO_PROBE = "/bench/peer/client-to-probe.json"
C_IC_NA_1, ACT = 100, 6

pytestmark = pytest.mark.usefixtures("probe_binary")


def _probe_frames(bench: Bench) -> list[tshark.Frame]:
    frames = tshark.decode(bench, "probe")
    assert tshark.errors(bench, "probe") == []
    return frames


def _sent_numbers(frames: list[tshark.Frame], source: str) -> list[int]:
    """N(S) of the I frames sent by `source`, in order, as they crossed the wire."""
    return [f.tx for f in frames if f.src == source and f.kind == "I"]


def _closed_after(frames: list[tshark.Frame], peer: str, number: int) -> bool:
    """The peer sent a FIN or a RST after the frame numbered `number`."""
    return any(
        f.src == peer and f.number > number and ("[FIN" in f.info or "[RST" in f.info)
        for f in frames
    )


def _run_probe(bench: Bench, scenario: str, address: str) -> str:
    result = bench.exec("probe", PROBE_BIN, scenario, address, timeout=300, check=False)
    assert result.returncode == 0, f"{scenario} failed:\n{result.stdout}{result.stderr}"
    assert f"PASS {scenario}" in result.stdout
    return result.stdout


def _run_listening_probe(bench: Bench, scenario: str) -> str:
    """Starts a controlled-side scenario, then the reference client connects to it."""
    proc = bench.exec_async("probe", PROBE_BIN, scenario, "0.0.0.0:2404")
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
    assert f"PASS {scenario}" in first + rest
    return first + rest


def test_d1_t1_closes_when_the_peer_never_confirms(bench_for):
    bench = bench_for()
    bench.up("ref-server", "probe")
    bench.wait_ready("ref-server")
    _run_probe(bench, "d1-fault-t1-silent", f"{SERVER}:2404")
    capture.stop(bench, "probe")
    frames = _probe_frames(bench)
    assert tshark.find(frames, src=PROBE, info="STARTDT act")


def test_d1_late_acknowledgement_within_t1_keeps_the_connection(bench_for):
    bench = bench_for()
    bench.up("ref-server", "probe")
    bench.wait_ready("ref-server")
    _run_probe(bench, "d1-fault-ack-within-t1", f"{SERVER}:2404")
    capture.stop(bench, "probe")
    frames = _probe_frames(bench)
    assert tshark.find(frames, src=SERVER, typeid=C_IC_NA_1, cause=10)


def test_d1_late_acknowledgement_beyond_t1_closes_on_t1(bench_for):
    bench = bench_for()
    bench.up("ref-server", "probe")
    bench.wait_ready("ref-server")
    _run_probe(bench, "d1-fault-ack-beyond-t1", f"{SERVER}:2404")
    capture.stop(bench, "probe")
    frames = _probe_frames(bench)
    assert tshark.find(frames, src=PROBE, typeid=C_IC_NA_1, cause=ACT)


def test_d1_dropped_frame_makes_the_server_close_on_a_disturbed_sequence(bench_for):
    bench = bench_for()
    bench.up("ref-server", "probe")
    bench.wait_ready("ref-server")
    _run_probe(bench, "d1-fault-dropped", f"{SERVER}:2404")
    capture.stop(bench, "probe")
    frames = _probe_frames(bench)
    # Our second frame (N(S) 1) never crossed the wire: the third one follows the first.
    assert _sent_numbers(frames, PROBE) == [0, 2]
    third = [f for f in frames if f.src == PROBE and f.kind == "I" and f.tx == 2][0]
    assert _closed_after(frames, SERVER, third.number)


def test_d1_duplicated_frame_makes_the_server_close_on_a_disturbed_sequence(bench_for):
    bench = bench_for()
    bench.up("ref-server", "probe")
    bench.wait_ready("ref-server")
    _run_probe(bench, "d1-fault-duplicated", f"{SERVER}:2404")
    capture.stop(bench, "probe")
    frames = _probe_frames(bench)
    # The same N(S) crossed the wire twice, and the server closed after the second copy.
    assert _sent_numbers(frames, PROBE) == [0, 0]
    second = [f for f in frames if f.src == PROBE and f.kind == "I"][-1]
    assert _closed_after(frames, SERVER, second.number)


def test_d2_t1_closes_when_our_frames_are_never_acknowledged(bench_for):
    bench = bench_for({"BENCH_CLIENT_CONFIG": TO_PROBE})
    bench.up("probe")
    _run_listening_probe(bench, "d2-fault-t1-silent")
    capture.stop(bench, "probe")
    frames = _probe_frames(bench)
    assert _sent_numbers(frames, PROBE) == [0, 1, 2]


def test_d2_late_acknowledgement_makes_the_peer_close_on_t1(bench_for):
    bench = bench_for({"BENCH_CLIENT_CONFIG": TO_PROBE})
    bench.up("probe")
    _run_listening_probe(bench, "d2-fault-ack-late")
    capture.stop(bench, "probe")
    frames = _probe_frames(bench)
    interrogation = [f for f in frames if f.src == CLIENT and f.typeids == (C_IC_NA_1,)][0]
    assert _closed_after(frames, CLIENT, interrogation.number)


def test_d2_dropped_frame_makes_the_peer_close_on_a_disturbed_sequence(bench_for):
    bench = bench_for({"BENCH_CLIENT_CONFIG": TO_PROBE})
    bench.up("probe")
    _run_listening_probe(bench, "d2-fault-dropped")
    capture.stop(bench, "probe")
    frames = _probe_frames(bench)
    assert _sent_numbers(frames, PROBE) == [0, 2]
    third = [f for f in frames if f.src == PROBE and f.kind == "I" and f.tx == 2][0]
    assert _closed_after(frames, CLIENT, third.number)


def test_d2_duplicated_frame_makes_our_session_close_on_a_sequence_error(bench_for):
    bench = bench_for({"BENCH_CLIENT_CONFIG": TO_PROBE})
    bench.up("probe")
    output = _run_listening_probe(bench, "d2-fault-duplicated")
    capture.stop(bench, "probe")
    frames = _probe_frames(bench)
    # The duplicate exists only in the relay: the wire carries the interrogation once.
    assert len([f for f in frames if f.src == CLIENT and f.typeids == (C_IC_NA_1,)]) == 1
    assert "closed on a sequence error" in output
