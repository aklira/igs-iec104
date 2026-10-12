# SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
#
# SPDX-License-Identifier: Apache-2.0
"""S5: a switchover under load. No data is lost when the station's redundancy group changes connection.

Two connections go to the same station: the first starts the data transfer, then the second starts it, which
supersedes the first (docs/igs104-server.md; §10.7). The changing points keep sending their changes throughout.
Each point's value rises by one at each change, so a gap in the sequence of a point's values is a change the
client never received. A repeated value is a change sent again on the new connection: allowed, and counted.

The check: no gap and no reordering for any point, on both connections.
"""

from __future__ import annotations

import pytest

from igs_bench import load, s5
from igs_bench.compose import Bench

PEER = s5.PEER_SERVICE
SERVER = "load-server"
SECONDS_PER_CONNECTION = 30
CHANGING_POINTS = 200


@pytest.fixture(scope="module")
def station():
    s5.build_server()
    load.write_points(changing=CHANGING_POINTS, stagger_ms=1)
    bench = Bench("s5-failover.pcap", profiles=("s5", "load"))
    bench.down()
    bench.up(SERVER, PEER)
    try:
        s5.wait_listening(bench, SERVER)
        yield bench
    finally:
        bench.down()


def _values(output: str) -> dict[int, list[tuple[float, str]]]:
    """The values of each point, in arrival order, with the connection that delivered them."""
    per_point: dict[int, list[tuple[float, str]]] = {}
    for line in load.parse_lines(output, "value "):
        fields = dict(part.split("=", 1) for part in line.split()[1:])
        per_point.setdefault(int(fields["ioa"]), []).append((float(fields["v"]), fields["conn"]))
    return per_point


def test_no_change_is_lost_when_the_connection_is_switched(station):
    command = [
        "java",
        "-cp",
        s5.PEER_CLASSPATH,
        "Load",
        "--host",
        s5.CONTAINER_LOAD_SERVER,
        "--port",
        "2404",
        "--common-address",
        s5.COMMON_ADDRESS,
        "--mode",
        "failover",
        "--seconds",
        str(SECONDS_PER_CONNECTION),
    ]
    result = station.exec(PEER, *command, timeout=SECONDS_PER_CONNECTION * 4 + 120, check=False)
    output = result.stdout
    assert result.returncode == 0, f"the load client failed:\n{output}{result.stderr}"
    assert "failover done" in output, output

    per_point = _values(output)
    assert per_point, "no change was received"
    # The second connection supersedes the first: nothing arrives on the first after the second has started.
    labels = [line.split()[-1] for line in load.parse_lines(output, "value ")]
    first_b = labels.index("conn=B")
    assert "conn=A" not in labels[first_b:], "the first connection still delivered after the switch"
    gaps: list[str] = []
    reordered: list[str] = []
    duplicates = 0
    on_a = on_b = 0
    for ioa, sequence in per_point.items():
        for value, conn in sequence:
            if conn == "A":
                on_a += 1
            else:
                on_b += 1
        for (previous, _), (current, _) in zip(sequence, sequence[1:]):
            step = current - previous
            if step == 0:
                duplicates += 1
            elif step < 0:
                reordered.append(f"ioa {ioa}: {previous} then {current}")
            elif abs(step - 1) > 1e-6:
                gaps.append(f"ioa {ioa}: {previous} then {current}")

    print(
        f"\nchanges: {on_a + on_b} ({on_a} on the first connection, {on_b} on the second); "
        f"repeated: {duplicates}; gaps: {len(gaps)}; reordered: {len(reordered)}"
    )
    assert on_a > 0, "the first connection delivered no change"
    assert on_b > 0, "the second connection delivered no change"
    assert not gaps, "a change was lost:\n" + "\n".join(gaps[:10])
    assert not reordered, "a change arrived out of order:\n" + "\n".join(reordered[:10])
