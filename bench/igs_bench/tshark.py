# SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
#
# SPDX-License-Identifier: Apache-2.0
"""Assertion helper: decodes captures with `tshark -T json` in the `capture` service.

The Wireshark dissector is an independent decoder: a capture that it parses
without malformed packets or error-level expert info is well formed.
"""

from __future__ import annotations

import json
import re
from dataclasses import dataclass

from .compose import Bench

FIELDS = (
    "frame.number",
    "frame.time_relative",
    "ip.src",
    "_ws.col.info",
    "iec60870_asdu.typeid",
    "iec60870_asdu.causetx",
    "iec60870_104.tx",
    "iec60870_104.rx",
)

# Frame kind in the Wireshark info column: "<- I (0,0)", "-> S (5)", "<- U (TESTFR act)".
_KIND = re.compile(r"(?:<-|->) ([ISU]) ")


@dataclass(frozen=True)
class Frame:
    number: int
    src: str
    info: str
    typeids: tuple[int, ...]
    causes: tuple[int, ...]
    time: float  # seconds since the first frame of the capture
    kind: str  # "I", "S", "U", or "" for a frame without APCI
    tx: int | None  # N(S) of an I frame
    rx: int | None  # N(R) of an I or S frame


def _path(bench: Bench) -> str:
    return f"/captures/{bench.capture_name}"


def decode(bench: Bench, service: str = "capture") -> list[Frame]:
    """Decode every APDU of the capture, which `service` wrote, into one Frame each.

    A packet can carry several APDUs (Wireshark joins their info with " | "). The
    per-APDU fields come as lists in the same order: the I frames give their
    N(S), N(R) and ASDU type, the S frames their N(R).
    """
    out = bench.exec(service, "tshark", "-r", _path(bench), "-T", "json", *(a for f in FIELDS for a in ("-e", f)))
    frames = []
    for packet in json.loads(out.stdout or "[]"):
        layers = packet["_source"]["layers"]
        number = int(layers["frame.number"][0])
        time_relative = float(layers["frame.time_relative"][0])
        src = layers.get("ip.src", [""])[0]
        parts = layers.get("_ws.col.info", [""])[0].split(" | ")
        typeids = iter(int(v) for v in layers.get("iec60870_asdu.typeid", []))
        causes = iter(int(v) for v in layers.get("iec60870_asdu.causetx", []))
        txs = iter(int(v) for v in layers.get("iec60870_104.tx", []))
        rxs = iter(int(v) for v in layers.get("iec60870_104.rx", []))
        for part in parts:
            match = _KIND.search(part)
            kind = match.group(1) if match else ""
            if kind == "I":
                typeid, cause = next(typeids, None), next(causes, None)
                frames.append(
                    Frame(
                        number=number,
                        time=time_relative,
                        src=src,
                        info=part,
                        typeids=(typeid,) if typeid is not None else (),
                        causes=(cause,) if cause is not None else (),
                        kind=kind,
                        tx=next(txs, None),
                        rx=next(rxs, None),
                    )
                )
            elif kind == "S":
                frames.append(
                    Frame(
                        number=number,
                        time=time_relative,
                        src=src,
                        info=part,
                        typeids=(),
                        causes=(),
                        kind=kind,
                        tx=None,
                        rx=next(rxs, None),
                    )
                )
            else:
                frames.append(
                    Frame(
                        number=number,
                        time=time_relative,
                        src=src,
                        info=part,
                        typeids=(),
                        causes=(),
                        kind=kind,
                        tx=None,
                        rx=None,
                    )
                )
    return frames


def errors(bench: Bench, service: str = "capture") -> list[str]:
    """Frames that tshark flags as malformed or with error-level expert info."""
    out = bench.exec(service, "tshark", "-r", _path(bench), "-Y", "_ws.malformed || _ws.expert.severity == error")
    return [line for line in out.stdout.splitlines() if line.strip()]


def find(frames: list[Frame], *, src: str, info: str | None = None, typeid: int | None = None, cause: int | None = None) -> list[Frame]:
    """Frames sent by `src` matching every given criterion."""
    return [
        f
        for f in frames
        if f.src == src
        and (info is None or info in f.info)
        and (typeid is None or typeid in f.typeids)
        and (cause is None or cause in f.causes)
    ]
