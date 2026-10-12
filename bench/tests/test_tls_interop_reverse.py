# SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
#
# SPDX-License-Identifier: Apache-2.0
"""X1, the other direction: igs104-client as the TLS client of a j60870 station over TLS (IEC 62351-3).

The station is the j60870 program TlsStation (bench/peer/j60870), with the identity of the crate's test fixtures.
The client is the command-line client of the station image. It checks the station's certificate against the trust
anchors it is given, and it presents its own certificate. The rows: an interrogation with the station's root
trusted (the point answered by the station is printed), and a client that trusts only its own certificate, which
is refused.
"""

from __future__ import annotations

import struct
import time

import pytest

from igs_bench import s5, tls
from igs_bench.compose import Bench

STATION = "tls-jstation"
CLIENT = "tls-client"
STATION_ADDRESS = "172.31.104.62:2404"
MATERIAL = "/bench/tls-material"
CLIENT_BINARY = "/usr/local/bin/igs104-client"
# The point the station answers with (TlsStation.POINT_ADDRESS, 100) and its value, 12.5 as an IEEE 754 single.
# The client prints the 32 bits of the value as an integer, so the test looks for that integer.
POINT_ADDRESS = "InformationObjectAddress(100)"
POINT_BITS = str(int.from_bytes(struct.pack(">f", 12.5), "big"))


def _client(roots: str) -> list[str]:
    return [
        CLIENT_BINARY,
        "--address",
        STATION_ADDRESS,
        "-c",
        "1",
        "--timeout",
        "10",
        "--tls-roots",
        f"{MATERIAL}/{roots}",
        "--tls-certificate",
        f"{MATERIAL}/client.pem",
        "--tls-key",
        f"{MATERIAL}/client.key",
        "interrogate",
    ]


@pytest.fixture(scope="module")
def bench():
    tls.prepare()
    instance = Bench("tls-interop-reverse.pcap", profiles=("tls",))
    instance.down()
    instance.run("build", "tls-server", "tls-peer", timeout=1800)
    instance.up(STATION, CLIENT)
    try:
        s5.wait_listening(instance, STATION)
        time.sleep(1)
        yield instance
    finally:
        instance.down()


def test_the_client_interrogates_the_tls_station_and_prints_its_point(bench):
    result = bench.exec(CLIENT, *_client("roots.pem"), timeout=120, check=False)
    assert result.returncode == 0, f"the interrogation failed:\n{result.stdout}{result.stderr}"
    assert POINT_ADDRESS in result.stdout and POINT_BITS in result.stdout, (
        f"the point is not printed:\n{result.stdout}"
    )


def test_the_client_refuses_a_station_it_does_not_trust(bench):
    # The client trusts only its own certificate: the station's certificate is not under it. The client
    # reports the failed connection, and it does not print the point.
    result = bench.exec(CLIENT, *_client("client.pem"), timeout=120, check=False)
    output = result.stdout + result.stderr
    assert result.returncode != 0, f"the client accepted an untrusted station:\n{output}"
    assert "connection failed" in output, f"the refusal is not a failed connection:\n{output}"
    assert POINT_ADDRESS not in output, output
