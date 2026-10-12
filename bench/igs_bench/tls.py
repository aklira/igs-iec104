# SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
#
# SPDX-License-Identifier: Apache-2.0
"""The key material of the TLS interop tests (X1).

The certificates and keys are the test fixtures of the crate (crates/igs-iec104/tests/fixtures/tls). They are
copied into bench/tls-material at test time, which git ignores. The copies are readable by the containers' users
(the fixtures' keys are owner-only, and the containers run as non-root users). They are test certificates of the
crate, not secrets. The PKCS12 files the Java peer reads are made here with the openssl command line.
"""

from __future__ import annotations

import os
import shutil
import subprocess
from pathlib import Path

from .compose import BENCH_DIR

FIXTURES = BENCH_DIR.parent / "crates" / "igs-iec104" / "tests" / "fixtures" / "tls"
MATERIAL = BENCH_DIR / "tls-material"
# The password of the PKCS12 copies of the test material. It protects test files only.
PASSWORD = "bench"
PEM_FILES = (
    "roots.pem",
    "server.pem",
    "server.key",
    "client.pem",
    "client.key",
    "rogue_client.pem",
    "rogue_client.key",
)


def prepare() -> Path:
    """Copies the test material into bench/tls-material and makes the PKCS12 keystores the peer reads."""
    MATERIAL.mkdir(exist_ok=True)
    os.chmod(MATERIAL, 0o755)
    for name in PEM_FILES:
        target = MATERIAL / name
        shutil.copyfile(FIXTURES / name, target)
        os.chmod(target, 0o644)
    _pkcs12(
        ["-export", "-in", "client.pem", "-inkey", "client.key", "-out", "client.p12"],
        cwd=MATERIAL,
    )
    _pkcs12(
        ["-export", "-in", "rogue_client.pem", "-inkey", "rogue_client.key", "-out", "rogue.p12"],
        cwd=MATERIAL,
    )
    _pkcs12(
        ["-export", "-in", "server.pem", "-inkey", "server.key", "-out", "server.p12"],
        cwd=MATERIAL,
    )
    return MATERIAL


def _pkcs12(arguments: list[str], cwd: Path) -> None:
    subprocess.run(
        ["openssl", "pkcs12", *arguments, "-passout", f"pass:{PASSWORD}"],
        cwd=cwd,
        check=True,
        capture_output=True,
    )
    for target in cwd.glob("*.p12"):
        os.chmod(target, 0o644)
