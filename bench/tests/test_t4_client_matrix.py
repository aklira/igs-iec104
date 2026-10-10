# SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
#
# SPDX-License-Identifier: Apache-2.0
"""T4: igs104-client against the reference server, one row per procedure of T2.

The client (bench/bin/igs104-client) runs inside the probe service, so the probe's
capture holds its traffic with the reference server (black box: the FledgePower
north plugin). A row names what the client must print and the exit status it must
return, and the answers that must cross the wire, as tshark decodes them: type ID,
cause of transmission and the P/N bit of a confirmation (101 7.2.3).

Twenty-seven rows are refusals by the reference server, which this bench cannot turn
into confirmations. Its plugin does not implement C_CI_NA_1 or C_RD_NA_1 (its log says
"unsupported command type"), and it answers both with cause 44. Its control path has
no south plugin connected, so it rejects every command with a negative activation
confirmation (its log says "received while south plugin is not connected"). These
rows check that the client reports the refusal; they do not show the procedure
working. docs/ai-log/T4.md sets this out for the maintainer's gate.

Reconnection is not in this matrix: restarting the reference container does not bring
its Fledge service back (docs/ai-log/T4.md), and T2 tests the reconnect policy on
loopback.

Values used as data: type IDs 1 and 9 (M_SP_NA_1, M_ME_NA_1), 45 to 50 and 58 to 63
(commands), 100 to 103 and 107 (interrogation, counter, read, clock, test); causes 5
(request), 6 (activation), 7 (activation confirmation), 10 (activation termination),
20 (interrogated by station), 44 (unknown type identification).
"""

from __future__ import annotations

import subprocess
import time
from dataclasses import dataclass
from typing import Union

import pytest

from igs_bench import capture, client, tshark
from igs_bench.compose import Bench

PROBE = "172.31.104.30"
SERVER = "172.31.104.10"
PORT = 2404
STATION = 45  # the common address of the reference server's exchanged data
CLIENT_BIN = client.CONTAINER_BINARY

M_SP_NA_1, M_ME_NA_1 = 1, 9
C_SC_NA_1, C_DC_NA_1, C_RC_NA_1 = 45, 46, 47
C_SE_NA_1, C_SE_NB_1, C_SE_NC_1 = 48, 49, 50
C_SC_TA_1, C_DC_TA_1, C_RC_TA_1 = 58, 59, 60
C_SE_TA_1, C_SE_TB_1, C_SE_TC_1 = 61, 62, 63
C_IC_NA_1, C_CI_NA_1, C_RD_NA_1, C_CS_NA_1, C_TS_TA_1 = 100, 101, 102, 103, 107
REQUEST, ACTIVATION, ACTIVATION_CONFIRMATION, ACTIVATION_TERMINATION = 5, 6, 7, 10
INTERROGATED_STATION, UNKNOWN_TYPE_ID = 20, 44

pytestmark = pytest.mark.usefixtures("client_binary")


@dataclass(frozen=True)
class Control:
    """A U frame: `src` sent the function `label`, as tshark prints it."""

    src: str
    label: str


@dataclass(frozen=True)
class Asdu:
    """An I frame: `src` sent an ASDU of this type, cause and P/N bit."""

    src: str
    typeid: int
    cause: int
    negative: bool = False


Fact = Union[Control, Asdu]


@dataclass(frozen=True)
class Row:
    name: str
    argv: tuple[str, ...]  # the procedure, after the global options
    exit: int
    message: str  # text the client prints; empty when any output will do
    facts: tuple[Fact, ...]
    address: str = f"{SERVER}:{PORT}"
    common: int = STATION


def ask(typeid: int, cause: int = ACTIVATION) -> Asdu:
    return Asdu(PROBE, typeid, cause)


def answer(typeid: int, cause: int, negative: bool = False) -> Asdu:
    return Asdu(SERVER, typeid, cause, negative)


def _command_rows() -> list[Row]:
    """Every command of T2, select and execute, without and with a time tag: 24 rows."""
    kinds = (
        ("single", ("single", "10005"), C_SC_NA_1, C_SC_TA_1),
        ("double", ("double", "10006"), C_DC_NA_1, C_DC_TA_1),
        ("step", ("step", "10007"), C_RC_NA_1, C_RC_TA_1),
        ("set-point-normalized", ("set-point", "10008", "--kind", "normalized", "--value", "1"), C_SE_NA_1, C_SE_TA_1),
        ("set-point-scaled", ("set-point", "10009", "--kind", "scaled", "--value", "1"), C_SE_NB_1, C_SE_TB_1),
        ("set-point-float", ("set-point", "10010", "--kind", "float", "--value", "1.5"), C_SE_NC_1, C_SE_TC_1),
    )
    rows = []
    for name, argv, plain, timed in kinds:
        for mode, flags, typeid in (
            ("select", ("--select",), plain),
            ("execute", (), plain),
            ("select-timed", ("--select", "--time"), timed),
            ("execute-timed", ("--time",), timed),
        ):
            rows.append(
                Row(
                    name=f"{name}-{mode}",
                    argv=(*argv, *flags),
                    exit=1,
                    message="refused",
                    facts=(ask(typeid), answer(typeid, ACTIVATION_CONFIRMATION, True)),
                )
            )
    return rows


ROWS = [
    Row(
        "start-and-stop",
        ("stop",),
        0,
        "transfer: Stopped",
        (
            Control(PROBE, "STARTDT act"),
            Control(SERVER, "STARTDT con"),
            Control(PROBE, "STOPDT act"),
            Control(SERVER, "STOPDT con"),
        ),
    ),
    Row(
        "general-interrogation",
        ("interrogate",),
        0,
        "",
        (
            ask(C_IC_NA_1),
            answer(C_IC_NA_1, ACTIVATION_CONFIRMATION),
            answer(M_SP_NA_1, INTERROGATED_STATION),
            answer(M_ME_NA_1, INTERROGATED_STATION),
            answer(C_IC_NA_1, ACTIVATION_TERMINATION),
        ),
    ),
    Row(
        "group-interrogation",
        ("interrogate", "--group", "1"),
        0,
        "",
        (
            ask(C_IC_NA_1),
            answer(C_IC_NA_1, ACTIVATION_CONFIRMATION),
            answer(C_IC_NA_1, ACTIVATION_TERMINATION),
        ),
    ),
    Row(
        "interrogation-with-another-address",
        ("interrogate",),
        1,
        "refused",
        (ask(C_IC_NA_1), answer(C_IC_NA_1, ACTIVATION_CONFIRMATION, True)),
        common=1,
    ),
    Row(
        "counter-interrogation",
        ("counter",),
        1,
        "refused",
        (ask(C_CI_NA_1), answer(C_CI_NA_1, UNKNOWN_TYPE_ID, True)),
    ),
    Row(
        "read",
        ("read", "672"),
        1,
        "refused",
        (ask(C_RD_NA_1, REQUEST), answer(C_RD_NA_1, UNKNOWN_TYPE_ID, True)),
    ),
    Row(
        "clock-synchronization",
        ("clock-sync",),
        0,
        "",
        (ask(C_CS_NA_1), answer(C_CS_NA_1, ACTIVATION_CONFIRMATION)),
    ),
    Row(
        "test-command",
        ("test",),
        0,
        "",
        (ask(C_TS_TA_1), answer(C_TS_TA_1, ACTIVATION_CONFIRMATION)),
    ),
    Row(
        "connection-refused",
        ("interrogate",),
        1,
        "connection",
        (),
        address=f"{SERVER}:{PORT + 1}",
    ),
    *_command_rows(),
]


@pytest.fixture(scope="module")
def matrix():
    """One reference server and one probe for the whole matrix: the server starts once."""
    capture.CAPTURE_DIR.mkdir(exist_ok=True)
    instance = Bench(f"t4-matrix-{time.strftime('%Y%m%dT%H%M%S')}.pcap")
    instance.down()
    instance.up("ref-server", "probe")
    instance.wait_ready("ref-server")
    try:
        yield instance
    finally:
        instance.down()


def _decode(bench: Bench) -> list[tshark.Frame]:
    """The probe's capture so far. tcpdump is still writing it, so a read may catch a packet half written."""
    for _ in range(5):
        try:
            return tshark.decode(bench, "probe")
        except subprocess.CalledProcessError:
            time.sleep(1)
    return tshark.decode(bench, "probe")


def _seen(frames: list[tshark.Frame], fact: Fact) -> bool:
    if isinstance(fact, Control):
        return bool(tshark.find(frames, src=fact.src, info=fact.label))
    found = tshark.find(frames, src=fact.src, typeid=fact.typeid, cause=fact.cause)
    return any(frame.negative == fact.negative for frame in found)


@pytest.mark.parametrize("row", ROWS, ids=lambda row: row.name)
def test_procedure_against_the_reference_server(matrix, row: Row):
    before = max((frame.number for frame in _decode(matrix)), default=0)
    result = matrix.exec(
        "probe",
        CLIENT_BIN,
        "-a",
        row.address,
        "-c",
        str(row.common),
        "-t",
        "8",
        *row.argv,
        timeout=120,
        check=False,
    )
    output = result.stdout + result.stderr
    assert result.returncode == row.exit, f"{row.name}: exit {result.returncode}\n{output}"
    assert row.message in output, f"{row.name}: no {row.message!r} in\n{output}"

    time.sleep(1)  # the last answer reaches the capture a moment after the client prints it
    frames = [frame for frame in _decode(matrix) if frame.number > before]
    assert tshark.errors(matrix, "probe") == []
    missing = [fact for fact in row.facts if not _seen(frames, fact)]
    assert not missing, f"{row.name}: not on the wire: {missing}"

