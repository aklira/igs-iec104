# X3-publish - Initial crates.io publication

## Prompt (translated)

Publish the igs-iec104 library on crates.io, using the API key in
~/Dev/igs-common/.env.

## Scope and authorization

The maintainer explicitly requested publication. Publish the existing 0.1.0
versions of igs-iec104-codec, igs-iec104-link and igs-iec104, in dependency order.
This is an initial development release. It does not complete X3 or approve 1.0:
C7, the remaining S5 checks and the independent release reviews remain pending.
No push or tag is authorized by this task.

## Preparation

- Add crate-specific READMEs, docs.rs links, keywords and a registry category.
- Include the full Apache-2.0 license in each package through a repository-local symlink.
- Mark the changelog as 0.1.0 and update the root README status.
- Add docs/RELEASE.md with the development release checklist and remaining 1.0 gates.
- Update Tokio 1.53.2 to 1.53.3, rustls 0.23.45 to 0.23.46 and rustls-webpki
  0.103.15 to 0.103.16 after cargo-deny reported security advisories.
- Require the patched Tokio and rustls versions in the public crate manifest.
- Inspect the package archives: 36 files for codec, 12 for link and 84 for
  the async crate. Each has a README and the full license; none contains
  the credential file or licensed standards.
- Use the API key only as a child-process environment variable; do not print
  it, persist it in Cargo credentials or pass it as a command-line argument.

## Validation and publication

The final standard gate passed:
- `cargo fmt --all --check`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo test --workspace --all-features`: 427 passed, 0 ignored, none failed.
- `cargo deny check`: advisories, bans, licenses and sources all pass. Existing
  duplicate-version and unused-license-allowance warnings remain non-blocking.

Additional release checks passed:
- `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features`
- `cargo publish --dry-run --allow-dirty --all-features -p igs-iec104-codec -p igs-iec104-link -p igs-iec104`

The preparation is committed before upload so each package records a clean source
commit. The publication receipt will be added after crates.io confirms all versions.
The initial development publication is tracked separately from the pending X3 gate.
