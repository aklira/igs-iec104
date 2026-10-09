# SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
#
# SPDX-License-Identifier: Apache-2.0

from __future__ import annotations

import time

import pytest

from igs_bench.capture import CAPTURE_DIR
from igs_bench.compose import Bench


@pytest.fixture
def bench(request):
    """A fresh bench per scenario; the capture is kept in bench/captures/."""
    CAPTURE_DIR.mkdir(exist_ok=True)
    name = f"{request.node.name}-{time.strftime('%Y%m%dT%H%M%S')}.pcap"
    instance = Bench(name)
    instance.down()
    try:
        yield instance
    finally:
        instance.down()
