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
- TLS (IEC 62351-3) over OpenSSL, the default feature `openssl`: the client and server options, the
  security events of Annex A, and the profile of the conformance tables, with every mandatory item
  negotiated by a test. The feature `rustls` is the alternative backend.
- Binaries `igs104-client`, a command-line controlling station, and `igs104-server`, a demo
  controlled station with simulated points.
- Examples `client`, `server`, `redundancy` and `tls`.
- An interop bench in `bench/`, with a client matrix against a reference server.

### Fixed

- `igs-iec104-link` reports a change of the transfer state as `Action::Transfer`, in order with the
  ASDUs of the same read. The change used to follow the whole read, so an application could receive
  an ASDU before the start of the data transfer, when the confirmation and the ASDU came in one read.
- The OpenSSL backend configures on OpenSSL 3.0, which rejects a signature algorithm list that names
  one scheme twice.

### Known gaps

- TLS has two backends: OpenSSL (default, feature `openssl`) and rustls (feature `rustls`). The OpenSSL
  backend renegotiates a TLS 1.2 session at the configured interval (default 12 hours). The rustls
  backend does not renegotiate, and it lacks the mandatory CBC and DHE suites of TLS 1.2, CCM, ffdhe2048,
  rsa_pss_pss_sha256 and the renegotiation (decision Q-025, `docs/ai-log/X1.md`).
- Neither backend yet applies the key update interval of clause 8.4, and the server does not yet check
  that the peer renegotiates in time (clause 7.4.5). The default build needs the OpenSSL development
  files (`libssl-dev`).
- The demo station `igs104-server` serves a built-in set of points, or the points of a TOML file given
  with `--points`.
- The interop, load and switchover tests under load are not done.

### Notes

- The minimum supported Rust version is 1.85. CI builds and tests the workspace on it.
