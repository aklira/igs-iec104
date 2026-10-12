# X1 — TLS options of the demo station `igs104-server`

## Prompt (summarised)

The TLS scenarios of the bench (X1, "done when") need a station that serves TLS as a separate process. The demo
station had no TLS option (`docs/igs104-server.md`). The maintainer chose option 1: add TLS options to the
binary.

## Summary

- `crates/igs-iec104/src/bin/igs104_server.rs`: three options, `--tls-roots`, `--tls-certificate` and `--tls-key`.
  Given together, they build the OpenSSL backend of the station and pass it to the server configuration. Given
  partly, or in a build without the `openssl` feature, the station refuses to start, with the reason.
- `crates/igs-iec104/src/bin/igs104_server/tls.rs`: builds the backend from the PEM files and prints each security
  event as it is raised (the same printer as the TLS example).
- `crates/igs-iec104/tests/server_binary.rs`: three tests. A TLS client that presents a certificate starts the data
  transfer on the TLS station; a plain client gets no transfer from it; the options come together.
- `docs/igs104-server.md`, `CHANGELOG.md`: the options and the statement "no TLS option" is removed.

## Decisions

- **The client certificate is required**, as in the library (`ServerConfig::with_tls`). The bench tests the
  station as the standard requires it; an option to relax it would be a separate decision.
- **The three options come together**, and the station refuses a partial set: a half-configured TLS station
  would otherwise serve plain IEC 104 without warning.
- **The build stays as it was**: the default features include `openssl`, so the TLS options are compiled in by
  default. The static musl build of the bench uses `--no-default-features`: its binary has no TLS, and refuses the
  options with a message. The bench's TLS image therefore needs a build with `openssl` (dynamic, in a container),
  which is the next step of X1.

## Verification

- `cargo test -p igs-iec104 --test server_binary`: 8 passed.
- The gate (fmt, clippy, the workspace tests, `cargo deny`) runs before the commit.

## The TLS interop of the bench

The bench runs the station with TLS in its own image, `bench/tls/station.Dockerfile`: built from the repository in a
pinned Rust image (1.99.0, by digest), with the OpenSSL backend, and run from a pinned Debian image with `libssl3t64`
and a non-root user (10001). The build context excludes the outputs, the bench state and `docs/spec`
(`.dockerignore`). The peer is the j60870 driver (`bench/peer/j60870`), now with TLS options: the trust anchors are
read from a PEM file, and the client certificate from a PKCS12 keystore.

`bench/tests/test_tls_interop.py`, seven tests, all passing:
- the interrogation, a single command and a counter interrogation over TLS, with a client certificate of the
  station's trust anchors: passed;
- a client certificate under another root: refused by the station's alert `unknown_ca`;
- a client without a certificate: refused by the alert `certificate_required`;
- a plain client: no transfer starts;
- the station raises `TLS_HS_SUCCESS` for the successful handshakes, and `TLS_NO_CA_MATCH` and `TLS_NO_PEER_CERT`
  for the refusals (clauses 6.1 and 6.4.3).

Decisions taken while running it:
- **The test material is copied** from the crate's fixtures into `bench/tls-material` (git-ignored), readable by the
  containers' users. The fixtures' keys are owner-only, and the containers run as other users. They are test
  certificates of the crate, not secrets.
- **The refusals are checked on their cause.** A first run took a failure to read a keystore for a refusal; the
  rows now name the alert or the message each case must meet, so another failure does not pass.
- **The trust anchors are read as PEM.** The PKCS12 files made by the OpenSSL command line carried the certificates,
  but Java did not accept them as trust anchors; the peer reads the PEM file directly.

Not covered yet: the other direction, where igs-iec104 is the TLS client of a j60870 station with TLS; this needs
the j60870 station with an SSL socket factory, and it is the remaining TLS interop of X1.
