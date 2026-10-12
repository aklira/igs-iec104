# SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
#
# SPDX-License-Identifier: Apache-2.0
"""The S5 interop matrix: the igs104-server station under test, and the second reference peer.

The station is the igs104-server binary built for the static target (no TLS, so no system library is
needed) and run in the `s5-server` service. The peer is the j60870 driver of bench/peer/j60870, run in
the `s5-peer` service, one procedure per `exec`. Its output is parsed here: one `OUTCOME` line per
procedure.
"""

from __future__ import annotations

import shutil
import subprocess
import time
from dataclasses import dataclass
from pathlib import Path

from .compose import BENCH_DIR, Bench
from .probe import TARGET

SERVER_BINARY = "igs104-server"
BIN_DIR = BENCH_DIR / "bin"
CONTAINER_SERVER = "172.31.104.40"
CONTAINER_LOAD_SERVER = "172.31.104.50"
CONTAINER_BINARY = f"/bench/bin/{SERVER_BINARY}"
PEER_SERVICE = "s5-peer"
SERVER_SERVICE = "s5-server"
PEER_CLASSPATH = "/opt/peer/classes:/opt/peer/j60870.jar"
COMMON_ADDRESS = "45"


def build_server() -> Path:
    """Build the station for the static target and copy it to bench/bin."""
    subprocess.run(
        [
            "cargo",
            "build",
            "--release",
            "--target",
            TARGET,
            "-p",
            "igs-iec104",
            "--bin",
            SERVER_BINARY,
            "--no-default-features",
        ],
        cwd=BENCH_DIR.parent,
        check=True,
    )
    BIN_DIR.mkdir(exist_ok=True)
    target = BENCH_DIR.parent / "target" / TARGET / "release" / SERVER_BINARY
    shutil.copy2(target, BIN_DIR / SERVER_BINARY)
    return BIN_DIR / SERVER_BINARY


@dataclass(frozen=True)
class Outcome:
    procedure: str
    kind: str  # passed, refused or timeout
    detail: str


def driver_command(procedure: str) -> list[str]:
    """The command that runs one procedure of the driver against the station under test."""
    return [
        "java",
        "-cp",
        PEER_CLASSPATH,
        "Driver",
        "--host",
        CONTAINER_SERVER,
        "--port",
        "2404",
        "--common-address",
        COMMON_ADDRESS,
        "--procedure",
        procedure,
    ]


def parse_outcome(output: str) -> Outcome:
    """The single OUTCOME line of a driver run: `OUTCOME <procedure> <kind> <detail>`."""
    lines = [line for line in output.splitlines() if line.startswith("OUTCOME ")]
    if len(lines) != 1:
        raise ValueError(f"expected one OUTCOME line, got {len(lines)}:\n{output}")
    _, procedure, kind, detail = lines[0].split(" ", 3)
    return Outcome(procedure, kind, detail)


def wait_listening(bench: Bench, service: str, timeout: float = 60) -> None:
    """Waits until the station in `service` listens on port 2404 (hexadecimal 0964 in /proc/net/tcp)."""
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        probe = bench.exec(service, "grep", "-q", ":0964 .* 0A ", "/proc/net/tcp", check=False)
        if probe.returncode == 0:
            return
        time.sleep(1)
    logs = bench.run("logs", service, check=False)
    raise TimeoutError(f"the station is not listening\n{logs.stdout}{logs.stderr}")
