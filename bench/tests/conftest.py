# SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
#
# SPDX-License-Identifier: Apache-2.0

from __future__ import annotations

import time

import pytest

from igs_bench import capture, probe
from igs_bench.compose import Bench


@pytest.fixture
def bench(request):
    """A fresh bench per scenario; the capture is kept in bench/captures/."""
    capture.CAPTURE_DIR.mkdir(exist_ok=True)
    name = f"{request.node.name}-{time.strftime('%Y%m%dT%H%M%S')}.pcap"
    instance = Bench(name)
    instance.down()
    try:
        yield instance
    finally:
        instance.down()


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


@pytest.fixture(scope="session")
def probe_binary():
    """The static L4/L5 probe, built once per session into bench/bin."""
    return probe.build()

