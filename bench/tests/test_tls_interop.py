# SPDX-FileCopyrightText: 2026 The igs-iec104 contributors
#
# SPDX-License-Identifier: Apache-2.0
"""X1: igs104-server over TLS, against the j60870 peer as a TLS client (IEC 62351-3).

The station is the image built from the repository (bench/tls/station.Dockerfile), with its identity and the
trust anchors of the crate's test fixtures. The peer connects with a client certificate that the station trusts,
and with the procedures of the S5 matrix. The rows that must fail are the client without a certificate, a client
whose certificate is signed by another root, and a plain client that speaks no TLS.

The station prints each security event of a handshake; the test checks that the successful handshakes raise the
notice (clause 6.1).
"""

from __future__ import annotations

import pytest

from igs_bench import s5, tls
from igs_bench.compose import Bench

PEER = "tls-peer"
SERVER = "tls-server"
SERVER_ADDRESS = "172.31.104.60"
PEER_CLASSPATH = "/opt/peer/classes:/opt/peer/j60870.jar"
MATERIAL = "/bench/tls-material"

# The refusal each client must meet, as the peer reports it: the station's fatal alert (unknown_ca for a
# certificate under another root, certificate_required without a certificate), or, on a plain connection,
# no transfer that starts.
REFUSALS = {
    "rogue": "unknown_ca",
    "none": "certificate_required",
    "plain": "the data transfer did not start",
}
# The security events the station raises for the refusals (clauses 6.4.3 of 62351-3).
REFUSAL_EVENTS = ("TLS_NO_CA_MATCH", "TLS_NO_PEER_CERT")

# procedure, client (the certificate the peer presents), expected kind of the outcome
ROWS = [
    ("interrogation", "client", "passed"),
    ("single", "client", "passed"),
    ("counter", "client", "passed"),
    ("interrogation", "rogue", "refused"),
    ("interrogation", "none", "refused"),
    ("interrogation", "plain", "refused"),
]


def _driver(procedure: str, client: str) -> list[str]:
    command = [
        "java",
        "-cp",
        PEER_CLASSPATH,
        "Driver",
        "--host",
        SERVER_ADDRESS,
        "--port",
        "2404",
        "--common-address",
        s5.COMMON_ADDRESS,
        "--procedure",
        procedure,
    ]
    if client == "plain":
        return command
    command += ["--tls-truststore", f"{MATERIAL}/roots.pem", "--tls-password", tls.PASSWORD]
    if client == "client":
        command += ["--tls-keystore", f"{MATERIAL}/client.p12"]
    elif client == "rogue":
        command += ["--tls-keystore", f"{MATERIAL}/rogue.p12"]
    return command


@pytest.fixture(scope="module")
def station():
    tls.prepare()
    s5.build_server()
    bench = Bench("tls-interop.pcap", profiles=("tls",))
    bench.down()
    bench.run("build", SERVER, PEER, timeout=1800)
    bench.up(SERVER, PEER)
    try:
        s5.wait_listening(bench, SERVER)
        yield bench
    finally:
        bench.down()


@pytest.mark.parametrize(("procedure", "client", "kind"), ROWS, ids=[f"{p}-{c}" for p, c, _ in ROWS])
def test_the_station_serves_tls_to_the_peer_and_refuses_the_rest(station, procedure, client, kind):
    result = station.exec(PEER, *_driver(procedure, client), timeout=180, check=False)
    if kind == "refused":
        # The refusal is the one the case means: a failed handshake, or no transfer on a plain connection.
        # Any other failure (a key the peer cannot read, for example) is not a pass.
        assert result.returncode != 0, f"the connection was not refused:\n{result.stdout}"
        assert "OUTCOME" not in result.stdout, result.stdout
        reason = REFUSALS[client]
        assert reason in result.stdout + result.stderr, (
            f"{client}: the refusal is not the expected one ({reason!r}):\n{result.stdout}{result.stderr}"
        )
        return
    assert result.returncode == 0, f"the driver failed:\n{result.stdout}{result.stderr}"
    outcome = s5.parse_outcome(result.stdout)
    assert outcome.kind == kind, f"{procedure}: {outcome}\n{result.stdout}"


def test_the_station_raises_the_notice_of_a_successful_handshake_and_the_events_of_the_refusals(station):
    logs = station.run("logs", SERVER, check=False).stdout
    assert "security event TLS_HS_SUCCESS" in logs, logs[-2000:]
    for event in REFUSAL_EVENTS:
        assert f"security event {event}" in logs, f"{event} is not raised:\n{logs[-2000:]}"
