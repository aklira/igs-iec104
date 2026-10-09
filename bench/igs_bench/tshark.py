# SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
#
# SPDX-License-Identifier: Apache-2.0
"""Assertion helper: decodes captures with `tshark -T json` in the `capture` service.

The Wireshark dissector is an independent decoder: a capture that it parses
without malformed packets or error-level expert info is well formed.
"""

from __future__ import annotations

import json
from dataclasses import dataclass

from .compose import Bench

FIELDS = (
    "frame.number",
    "ip.src",
    "_ws.col.info",
    "iec60870_asdu.typeid",
    "iec60870_asdu.causetx",
)


@dataclass(frozen=True)
class Frame:
    number: int
    src: str
    info: str
    typeids: tuple[int, ...]
    causes: tuple[int, ...]


def _path(bench: Bench) -> str:
    return f"/captures/{bench.capture_name}"


def decode(bench: Bench) -> list[Frame]:
    """Decode every frame of the capture into the fields of FIELDS."""
    out = bench.exec("capture", "tshark", "-r", _path(bench), "-T", "json", *(a for f in FIELDS for a in ("-e", f)))
    frames = []
    for packet in json.loads(out.stdout or "[]"):
        layers = packet["_source"]["layers"]
        frames.append(
            Frame(
                number=int(layers["frame.number"][0]),
                src=layers.get("ip.src", [""])[0],
                info=layers.get("_ws.col.info", [""])[0],
                typeids=tuple(int(v) for v in layers.get("iec60870_asdu.typeid", [])),
                causes=tuple(int(v) for v in layers.get("iec60870_asdu.causetx", [])),
            )
        )
    return frames


def errors(bench: Bench) -> list[str]:
    """Frames that tshark flags as malformed or with error-level expert info."""
    out = bench.exec("capture", "tshark", "-r", _path(bench), "-Y", "_ws.malformed || _ws.expert.severity == error")
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
