# Release checklist

## Initial development release: 0.1.0

The maintainer explicitly requested publication of the current library on crates.io.
This authorizes publishing 0.1.0 and its two internal dependencies. It does not
approve or complete the 1.0 release gates.

- Publish only `igs-iec104-codec`, `igs-iec104-link` and `igs-iec104`.
- Keep all three versions at 0.1.0; publish dependencies before their dependants.
- Run the standard gate: formatting, Clippy, workspace tests and cargo-deny.
- Verify package contents, license files and crate READMEs.
- Run `cargo publish --dry-run` for all three crates before upload.
- Read the requested API key into the publication process environment only.
- Verify that crates.io serves each uploaded version.
- Commit the preparation and publication record locally. Do not push or tag.

## Current publication status

Preparation checks pass. On 2026-11-07, crates.io rejected the first upload because
the account has no verified email address. No crate was published. Verify the
account email at https://crates.io/settings/profile, then retry publication.
See docs/ai-log/X3-publish.md for the checks and upload result.

## Remaining 1.0 gates

The following remain pending and are not certified by the 0.1.0 publication:

- C7: the maintainer's 24-hour fuzzing campaign.
- S5: load and redundancy switchover tests under load.
- Coverage thresholds: at least 85% codec and 75% link/server.
- TLS interoperability scenarios with the reference peer.
- X3: independent similarity check, legal review and final maintainer approval.

See the implementation plan and the C7, S5 and X1 AI logs for the recorded gaps.
