# Interop bench

Scenarios that run igs-iec104 against reference IEC 60870-5-104 peers. The
reference peers run as separate containers and are used as **black boxes
only** (IMPLEMENTATION.md section 1, rule 1): the bench configures them,
talks to them over TCP and inspects captures. Nothing from their source code
enters this repository.

## Reference peers

| Role | Container | Address | What runs |
| --- | --- | --- | --- |
| Reference server | `ref-server` | 172.31.104.10:2404 | FledgePower, IEC 104 north plugin |
| Reference client | `ref-client` | 172.31.104.21 | FledgePower, IEC 104 south plugin |

Both use the maintainer-approved image
`ghcr.io/aklira/fledgepower/fledge:v1.2.3`, pinned by digest in
`docker-compose.yml`. Fledge and FledgePower are Apache-2.0; their IEC 104
plugins are built on a third-party IEC 104 stack that is GPLv3 or
commercial. The bench only runs that image as a separate process and never
links it, so the licence does not reach this repository.

Each peer is started by `peer/bootstrap.sh`, not by the image's own start
script. The bootstrap starts Fledge and creates a single service from
`peer/server.json` or `peer/client.json`. It writes `/tmp/bench-ready` once
the service is running.

## Capture and decoding

The `capture` container (netshoot, pinned by digest) shares the network
namespace of `ref-server`, so it sees every frame the server exchanges.
tcpdump writes `captures/<scenario>-<timestamp>.pcap`. The assertion helpers
decode that file with `tshark -T json` inside the same container, so the
host needs neither tcpdump nor tshark. Captures are kept after a run and are
ignored by git.

## Requirements

- Docker with the compose plugin, or Podman with `podman-compose`. Set
  `BENCH_COMPOSE` to force one, for example `BENCH_COMPOSE=podman-compose`.
- Python 3.10 or later with pytest.
- The subnet 172.31.104.0/24 must be free on the host.

## Running

From the `bench/` directory:

```
python3 -m pip install -e .
python3 -m pytest
```

A scenario brings the bench up, runs, then tears everything down (`down -v`).
The first run pulls the images (about 600 MB).

## Layout

| Path | Purpose |
| --- | --- |
| `docker-compose.yml` | reference peers and capture container |
| `peer/` | Fledge bootstrap and the service definitions of the peers |
| `igs_bench/compose.py` | compose engine selection, up/down/exec, readiness wait |
| `igs_bench/capture.py` | stops tcpdump and returns the capture path |
| `igs_bench/tshark.py` | `tshark -T json` decoding, error check, frame matching |
| `tests/` | scenarios, one pytest file per task |

## Scenarios

| File | Task | What it checks |
| --- | --- | --- |
| `tests/test_f4_reference_gi.py` | F4 | the reference client starts data transfer on the reference server and runs a general interrogation (activation, confirmation, termination); the capture has no malformed frame and no error-level expert info |
