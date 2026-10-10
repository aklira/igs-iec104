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
- The OpenSSL backend ends a TLS 1.2 session whose renegotiation the peer does not answer in time, or
  refuses, with the alarm TLS_NO_RENEG (clause 7.4.5; the time is D-015 in PROVENANCE.md).
- The OpenSSL backend updates the keys of a TLS 1.3 session at the key update interval (default 6 hours,
  `TlsSettings::with_key_update`), and ends the session with the alarm TLS_NO_SK_UPDATE when the peer
  does not answer the KeyUpdate in time (clause 8.4; the time is D-016 in PROVENANCE.md).
- The OpenSSL backend raises 25 of the 36 security events of Annex A, and tests each one of them. Among
  them: the versions and the signature algorithms of a ClientHello, a CRL that is missing or expired
  (warnings; the session goes on, D-017), a SHA-1 certificate (D-018), RSA keys below 2 048 bits (the
  policy of D-019), and a signature that does not verify.
- The client renegotiates at its interval only when the station has not asked for a renegotiation in it
  (clause 7.4.5).
- `igs_iec104::answer::judge` tells whether an ASDU of a station is the answer to a request: its
  confirmation or termination, or a refusal (a negative confirmation, or a cause of 44 to 47). The client
  binary `igs104-client` uses it, and now matches the information object address of the answer too.
- An interop bench in `bench/`, with a client matrix against a reference server.

### Fixed

- `igs-iec104-link` reports a change of the transfer state as `Action::Transfer`, in order with the
  ASDUs of the same read. The change used to follow the whole read, so an application could receive
  an ASDU before the start of the data transfer, when the confirmation and the ASDU came in one read.
- The OpenSSL backend configures on OpenSSL 3.0, which rejects a signature algorithm list that names
  one scheme twice.
- A station does not send the answers of a closed connection on the connection that takes over. A
  controlling station took such an answer for the answer to its new request: a select could be
  refused by the confirmation of an earlier execution (D-021 in PROVENANCE.md). The events are
  still sent again.
- A station renegotiates a TLS 1.2 session more than once with a client certificate: the station sets a
  session ID context, which OpenSSL needs for the second renegotiation.

### Known gaps

- TLS has two backends: OpenSSL (default, feature `openssl`) and rustls (feature `rustls`). The OpenSSL
  backend renegotiates a TLS 1.2 session at the configured interval (default 12 hours). The rustls
  backend does not renegotiate, and it lacks the mandatory CBC and DHE suites of TLS 1.2, CCM, ffdhe2048,
  rsa_pss_pss_sha256 and the renegotiation (decision Q-025, `docs/ai-log/X1.md`).
- The rustls backend does not update the keys of a TLS 1.3 session (clause 8.4); the OpenSSL backend
  does. The default build needs the OpenSSL development files (`libssl-dev`).
- The demo station `igs104-server` serves a built-in set of points, or the points of a TOML file given
  with `--points`.
- The interop, load and switchover tests under load are not done.
- The TLS scenarios of the bench are not written: the reference plugin's TLS configuration is not in the
  repository (`docs/ai-log/X1.md`).
- Eleven of the 36 security events of Annex A are not raised by the OpenSSL backend, because the feature
  that triggers them is not implemented or not offered (`docs/ai-log/X1.md`).

### Notes

- The minimum supported Rust version is 1.85. CI builds and tests the workspace on it.
