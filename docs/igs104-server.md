# `igs104-server`: a demo controlled station

`igs104-server` is a controlled station of IEC 60870-5-104 for trials and for the interop bench. It serves
a set of points, changes their values as a simulation says, and accepts the commands of the points that
take them. It is not a production station: it has no TLS, and its points are fixed in the binary.

## Running it

```bash
cargo run -p igs-iec104 --bin igs104-server -- --bind 127.0.0.1:2404 -c 1
```

| Option | Meaning | Default |
| --- | --- | --- |
| `-b`, `--bind ADDR` | the address the station listens on (the standard port is 2404) | `0.0.0.0:2404` |
| `-c`, `--common-address N` | the common address of the station, 1 to 65534 (65535 is the global address and is refused) | `1` |
| `--for SECONDS` | stop after this many seconds; without it the station runs until the process is stopped | none |

The station prints its address when it starts, and a line for each command it accepts.

## What it does

- **Connections.** All connections of the station form one redundancy group (§10.2): one of them is
  started at a time, and a start on another connection closes the others that are not stopped (§10.7).
  Events raised while no connection is started wait for the next one.
- **Interrogation and reads.** The station answers the general interrogation and the group interrogations
  with its points, a counter interrogation with its counters, and a read with the point at the address.
  A time-tagged point is reported with its plain type in an interrogation, as the profile allows.
- **Clock and test.** A clock synchronization is confirmed with the station's time from before it; the
  station keeps its own clock. A test command is confirmed with the request.
- **Commands.** Select-before-operate: a select is kept for 30 s, and an execution is accepted only for a
  kept selection of the same type and address. A command to a point that takes none is refused with cause
  47 (unknown information object address). The station prints each command it accepts.
- **Simulated changes.** A point that has a simulation changes its value at each period, and the change is
  sent to the started connection as a spontaneous ASDU (cause 3). A simulation is a function of the step
  number, so a run is repeatable.

## The demo points

| Address | Type | Time tag | Group | Commands | Simulation |
| --- | --- | --- | --- | --- | --- |
| 1 | single point (M_SP_NA_1) | no | 1 | yes | toggles every 5 s |
| 2 | single point (M_SP_TB_1) | yes | none | no | toggles every 2 s |
| 3 | double point (M_DP_NA_1) | no | 1 | yes | toggles every 3 s |
| 4 | normalized value (M_ME_NA_1) | no | 1 | no | ramps by 1000 every 1 s |
| 5 | scaled value (M_ME_NB_1) | no | 1 | no | ramps by -700 every 1 s |
| 6 | short float (M_ME_NC_1 or M_ME_TF_1) | yes | 1 | no | ramps by 3 every 2 s |
| 7 | integrated total (M_IT_NA_1) | no | 2 | no | counts by 1 every 10 s |
| 8 | single point (M_SP_NA_1) | no | 2 | no | constant, off |

The ramps wrap around the range of a 16-bit value. Point 6 is time-tagged: its changes are M_ME_TF_1 with the
time of the change, and an interrogation reports it with the plain type M_ME_NC_1.

## Not yet

The points cannot be changed without rebuilding the binary. The plan asks for a TOML file of points (S4):
that needs a TOML parser, which is not in the plan's list of dependencies, so it waits for a maintainer's
approval (see `docs/ai-log/S4.md`).
