# `igs104-server`: a demo controlled station

`igs104-server` is a controlled station of IEC 60870-5-104 for trials and for the interop bench. It serves
a set of points, changes their values as a simulation says, and accepts the commands of the points that
take them. It is not a production station: it has no TLS option.

## Running it

```bash
cargo run -p igs-iec104 --bin igs104-server -- --bind 127.0.0.1:2404 -c 1
```

To serve other points, give a points file (see below):

```bash
cargo run -p igs-iec104 --bin igs104-server -- --bind 127.0.0.1:2404 -c 1 --points docs/igs104-server.points.toml
```

| Option | Meaning | Default |
| --- | --- | --- |
| `-b`, `--bind ADDR` | the address the station listens on (the standard port is 2404) | `0.0.0.0:2404` |
| `-c`, `--common-address N` | the common address of the station, 1 to 65534 (65535 is the global address and is refused) | `1` |
| `--for SECONDS` | stop after this many seconds; without it the station runs until the process is stopped | none |
| `-p`, `--points FILE` | a TOML file with the points of the station | the built-in demo points |

The station prints its address when it starts, and a line for each command it accepts. A points file that
cannot be read or checked stops the station at start, with the reason.

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

## The points file

A points file is TOML. Each `[[point]]` table is one information object:

| Key | Meaning | Default |
| --- | --- | --- |
| `address` | the information object address, 1 to 16777215, used once in the file | required |
| `kind` | `single`, `double`, `normalized`, `scaled`, `float` or `counter` | required |
| `stamped` | `true` for the time-tagged type of the kind (the changes carry their time) | `false` |
| `group` | the group, 1 to 16, for the group interrogations | none |
| `command` | `true` when the station accepts commands to the point; a counter takes none | `false` |
| `simulation` | `constant`, `toggle`, `ramp` or `count` | `constant` |
| `period_ms` | the time between two changes, in milliseconds above zero; every motion but `constant` needs it | none |
| `increment` | the change of the raw value at each step (`ramp`, a 32-bit signed number) or the increase of the count (`count`, 0 to 2^32-1); `toggle` and `constant` take none | none |

The kinds take these motions: `single` and `double` toggle; `normalized`, `scaled` and `float` ramp; `counter`
counts. A key that is not in the table above is an error, and so is a number that the motion does not use.
`docs/igs104-server.points.toml` is the built-in demo set written as a file: the station serves the same
points with `--points` as without it, and a test checks that the two are equal.

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

The station has no TLS option, and it does not reload its points file while it runs: restart it to change
the points.
