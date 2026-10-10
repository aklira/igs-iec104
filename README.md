# igs-iec104

Industrial-grade IEC 60870-5-104 protocol stack for SCADA and telecontrol, written in Rust.

**Status: pre-release (0.1.0, not published).** The codec, the link session, the async client and
controlled station with their redundancy groups, and the two binaries are implemented and tested.
TLS (IEC 62351-3) runs over rustls (default) or OpenSSL (feature `openssl`); only the OpenSSL
backend renegotiates TLS 1.2 sessions. The demo station `igs104-server` serves a built-in set of points, or the points of a
TOML file given with `--points`. See [CHANGELOG.md](CHANGELOG.md) for the details.

## Quick start

Run the tests of the workspace:

```bash
cargo test --workspace --all-features
```

Start the demo controlled station. It listens on 127.0.0.1:2404 and has the common address 1:

```bash
cargo run --example server -- 127.0.0.1:2404 1
```

In another terminal, connect a controlling station, start the data transfer and ask for a general
interrogation. The client prints every event until the interrogation is terminated:

```bash
cargo run --example client -- 127.0.0.1:2404 1
```

Keep a controlling station connected to two stations at once, and let it switch when the started
connection is lost:

```bash
cargo run --example redundancy -- 127.0.0.1:2404 127.0.0.1:2405
```

The examples are in [`crates/igs-iec104/examples/`](crates/igs-iec104/examples/).

The TLS example runs a secured station and a secured controlling station in one process. It takes
five PEM files (the trust anchors, the certificate and key of the station, and those of the
controlling station). The test material of the crate works:

```bash
cargo run --example tls -- crates/igs-iec104/tests/fixtures/tls/roots.pem \
    crates/igs-iec104/tests/fixtures/tls/server.pem crates/igs-iec104/tests/fixtures/tls/server.key \
    crates/igs-iec104/tests/fixtures/tls/client.pem crates/igs-iec104/tests/fixtures/tls/client.key
```

## Binaries

- `igs104-client`: a command-line controlling station, for interrogation, commands, clock
  synchronization and more. Run `cargo run --bin igs104-client -- --help`.
- `igs104-server`: the demo controlled station, described in
  [`docs/igs104-server.md`](docs/igs104-server.md). Run `cargo run --bin igs104-server -- --help`.

## Documentation

The API documentation is built with `cargo doc --workspace --no-deps --open`. Every public item of
the library crates is documented. CI builds the documentation with warnings as errors, so a broken
link fails the build.

## Minimum supported Rust version

The minimum supported Rust version (MSRV) is 1.85, the `rust-version` of the workspace. CI builds
and tests the workspace on that version. A change of the MSRV is recorded in
[CHANGELOG.md](CHANGELOG.md).

## Licence and standards

The code is licensed under the Apache License 2.0, see [LICENSE](LICENSE). The IEC standards are
licensed separately and are not included in this repository. The code cites their clauses and does
not copy their text.

## Project records

The implementation is driven task by task: see `IMPLEMENTATION.md` at the root of the repository.
The AI agent records each task prompt and summary, plus any open question where the standards are
silent or ambiguous, in [`docs/ai-log/`](docs/ai-log/).
