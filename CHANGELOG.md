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
- TLS (IEC 62351-3) over rustls, as the default feature: the client and server options, the
  security events of Annex A, and the profile of the conformance tables.
- Binaries `igs104-client`, a command-line controlling station, and `igs104-server`, a demo
  controlled station with simulated points.
- Examples `client`, `server`, `redundancy` and `tls`.
- An interop bench in `bench/`, with a client matrix against a reference server.

### Known gaps

- The OpenSSL backend of TLS (optional feature) is not implemented yet. The rustls backend does
  not offer every mandatory item of the conformance tables (see `docs/ai-log/X1.md`).
- The demo station `igs104-server` serves a built-in set of points, or the points of a TOML file given
  with `--points`.
- The interop, load and switchover tests under load are not done.

### Notes

- The minimum supported Rust version is 1.85. CI builds and tests the workspace on it.
