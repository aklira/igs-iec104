# SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
#
# SPDX-License-Identifier: Apache-2.0
"""Capture helper: tcpdump runs in the `capture` service, see docker-compose.yml."""

from __future__ import annotations

import time
from pathlib import Path

from .compose import BENCH_DIR, Bench

CAPTURE_DIR = BENCH_DIR / "captures"


def stop(bench: Bench) -> Path:
    """Stop tcpdump so that the capture file is complete; return its host path."""
    bench.exec("capture", "pkill", "-INT", "tcpdump", check=False)
    for _ in range(20):
        if bench.exec("capture", "pgrep", "tcpdump", check=False).returncode != 0:
            break
        time.sleep(0.5)
    return CAPTURE_DIR / bench.capture_name
