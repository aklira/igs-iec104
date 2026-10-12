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
