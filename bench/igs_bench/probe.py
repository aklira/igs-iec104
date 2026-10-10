# SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
#
# SPDX-License-Identifier: Apache-2.0
"""The L4 probe: built here as a static binary, run inside the `probe` service."""

from __future__ import annotations

import shutil
import subprocess
from pathlib import Path

from .compose import BENCH_DIR

TARGET = "x86_64-unknown-linux-musl"
BIN_DIR = BENCH_DIR / "bin"
BINARY = "igs-iec104-probe"
# Path of the binary inside the probe container (bench/bin is mounted there).
CONTAINER_BINARY = f"/bench/bin/{BINARY}"


def build() -> Path:
    """Build the probe for the static target and copy it to bench/bin."""
    subprocess.run(
        ["cargo", "build", "--release", "--target", TARGET, "-p", BINARY],
        cwd=BENCH_DIR.parent,
        check=True,
    )
    BIN_DIR.mkdir(exist_ok=True)
    target = BENCH_DIR.parent / "target" / TARGET / "release" / BINARY
    shutil.copy2(target, BIN_DIR / BINARY)
    return BIN_DIR / BINARY
