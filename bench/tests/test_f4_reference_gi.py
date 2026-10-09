# SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
#
# SPDX-License-Identifier: Apache-2.0
"""F4: the reference client starts data transfer on the reference server and
runs a general interrogation; the capture must decode cleanly in tshark.

This exercises the bench only (no igs-iec104 code yet). Values used as data:
type ID 100 is C_IC_NA_1, causes 6 / 7 / 10 are activation, activation
confirmation and activation termination (docs/spec/104/data/type_ids.yaml).
U-format functions are matched on the labels tshark prints.
"""

from __future__ import annotations

import subprocess
import time

from igs_bench import capture, tshark

SERVER = "172.31.104.10"
CLIENT = "172.31.104.21"
C_IC_NA_1 = 100
ACT, ACTCON, ACTTERM = 6, 7, 10


def _wait_gi_terminated(bench, timeout: float = 90) -> None:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        try:
            frames = tshark.decode(bench)
        except subprocess.CalledProcessError:
            frames = []  # capture still being written
        if tshark.find(frames, src=SERVER, typeid=C_IC_NA_1, cause=ACTTERM):
            return
        time.sleep(2)
    raise TimeoutError("no general interrogation termination from the reference server")


def test_reference_client_startdt_and_gi(bench):
    bench.up("ref-server", "capture")
    bench.wait_ready("ref-server")
    bench.up("ref-client")
    bench.wait_ready("ref-client")

    _wait_gi_terminated(bench)
    pcap = capture.stop(bench)
    assert pcap.is_file()

    assert tshark.errors(bench) == []
    frames = tshark.decode(bench)
    assert tshark.find(frames, src=CLIENT, info="STARTDT act")
    assert tshark.find(frames, src=SERVER, info="STARTDT con")
    assert tshark.find(frames, src=CLIENT, typeid=C_IC_NA_1, cause=ACT)
    assert tshark.find(frames, src=SERVER, typeid=C_IC_NA_1, cause=ACTCON)
    assert tshark.find(frames, src=SERVER, typeid=C_IC_NA_1, cause=ACTTERM)
