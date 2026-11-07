# igs-iec104-link

Sans-I/O APCI session state machine for IEC 60870-5-104.

The session consumes events and returns actions for its caller to execute. It handles
sequence numbers, acknowledgements, send and receive windows, timers, and data transfer
states. This is the link layer of [igs-iec104](https://crates.io/crates/igs-iec104).

## Usage

Add `igs-iec104-link = "0.1.0"` to your dependencies.

See the [API documentation](https://docs.rs/igs-iec104-link) and the
[project repository](https://github.com/aklira/igs-iec104).

## Status and license

Initial development release. The 1.0 release gates are still pending; see the
[release checklist](https://github.com/aklira/igs-iec104/blob/main/docs/RELEASE.md).

Apache-2.0. The licensed IEC standards are not distributed with this crate.
