# SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
#
# SPDX-License-Identifier: Apache-2.0
"""Thin wrapper over `docker compose` or `podman-compose` for the bench."""

from __future__ import annotations

import os
import shlex
import shutil
import subprocess
import time
from pathlib import Path

BENCH_DIR = Path(__file__).resolve().parent.parent
PROJECT = "igsbench"


def _engine() -> list[str]:
    """Compose command: $BENCH_COMPOSE, else docker compose, else podman-compose."""
    override = os.environ.get("BENCH_COMPOSE")
    if override:
        return shlex.split(override)
    if shutil.which("docker"):
        return ["docker", "compose"]
    if shutil.which("podman-compose"):
        return ["podman-compose"]
    raise RuntimeError("no compose engine found: install docker or podman-compose")


class Bench:
    """One running instance of bench/docker-compose.yml."""

    def __init__(self, capture_name: str) -> None:
        self.capture_name = capture_name
        self._cmd = [*_engine(), "-p", PROJECT, "-f", str(BENCH_DIR / "docker-compose.yml")]
        self._env = {**os.environ, "BENCH_CAPTURE": capture_name}

    def run(self, *args: str, timeout: float = 120, check: bool = True) -> subprocess.CompletedProcess[str]:
        return subprocess.run(
            [*self._cmd, *args],
            cwd=BENCH_DIR,
            env=self._env,
            capture_output=True,
            text=True,
            timeout=timeout,
            check=check,
        )

    def up(self, *services: str) -> None:
        self.run("up", "-d", *services, timeout=300)

    def down(self) -> None:
        self.run("down", "-v", "-t", "2", timeout=300, check=False)

    def exec(self, service: str, *args: str, timeout: float = 60, check: bool = True) -> subprocess.CompletedProcess[str]:
        return self.run("exec", "-T", service, *args, timeout=timeout, check=check)

    def wait_ready(self, service: str, timeout: float = 240) -> None:
        """Wait for the peer bootstrap to report its Fledge service running."""
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            if self.exec(service, "test", "-f", "/tmp/bench-ready", check=False).returncode == 0:
                return
            time.sleep(2)
        logs = self.run("logs", service, check=False)
        raise TimeoutError(f"{service} not ready after {timeout}s\n{logs.stdout}{logs.stderr}")
