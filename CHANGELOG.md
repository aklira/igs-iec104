# Changelog

All notable changes to this project are recorded in this file. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Added

- `igs-iec104-codec`: APCI framing of the I, S and U formats; the ASDUs of the 104 profile, with
  their information elements and time tags; the profile rules. The profile values are generated
  from the profile data.
- `igs-iec104-link`: the sans-I/O APCI session, with sequence numbers, the windows k and w, the
  timers t0 to t3, STARTDT and STOPDT, and the transfer states.
- `igs-iec104`: the async client `Client`, with reconnection and the procedures of clause 7; the
  async controlled station `Server`, with its process image and its answers to the procedures; the
  redundancy groups on both sides (`RedundantClient` and the redundant controlled station).
- Binaries `igs104-client`, a command-line controlling station, and `igs104-server`, a demo
  controlled station with simulated points.
- Examples `client`, `server` and `redundancy`.
- An interop bench in `bench/`, with a client matrix against a reference server.

### Known gaps

- TLS (IEC 62351-3) is not implemented yet.
- `igs104-server` serves a built-in set of points; reading them from a TOML file is not available.
- The interop, load and switchover tests under load are not done.

### Notes

- The minimum supported Rust version is 1.85. CI builds and tests the workspace on it.
