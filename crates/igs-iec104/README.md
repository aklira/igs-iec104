# igs-iec104

Async IEC 60870-5-104 client and server for SCADA and telecontrol, written in Rust.

Includes Tokio-based clients and servers, redundancy groups, a process image, and
the `igs104-client` and `igs104-server` command-line programs.

## Usage

Add `igs-iec104 = "0.1.0"` to your dependencies.

Start with [Client](https://docs.rs/igs-iec104/latest/igs_iec104/client/struct.Client.html)
or [Server](https://docs.rs/igs-iec104/latest/igs_iec104/server/struct.Server.html).
See the [examples](https://github.com/aklira/igs-iec104/tree/main/crates/igs-iec104/examples)
and [supported procedures](https://github.com/aklira/igs-iec104/blob/main/docs/capabilities.md).

OpenSSL is the default TLS backend and requires its development files
(`libssl-dev` on Debian/Ubuntu). Use `default-features = false` for a build without
TLS, or add `features = ["rustls"]` with default features disabled for the
alternative backend. The rustls backend has profile gaps described in the
[changelog](https://github.com/aklira/igs-iec104/blob/main/CHANGELOG.md).

## Status and license

Initial development release. The 1.0 release gates are still pending; see the
[release checklist](https://github.com/aklira/igs-iec104/blob/main/docs/RELEASE.md).

Apache-2.0. The licensed IEC standards are not distributed with this crate.
