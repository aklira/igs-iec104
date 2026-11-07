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

Published and verified on crates.io on 2026-11-07:
- https://crates.io/crates/igs-iec104-codec/0.1.0
- https://crates.io/crates/igs-iec104-link/0.1.0
- https://crates.io/crates/igs-iec104/0.1.0

The account email verification initially blocked publication; the maintainer
confirmed it was completed, and the retry succeeded. All three versions are
available and are not yanked. See docs/ai-log/X3-publish.md for the checks and receipt.

## Automatic publishing for future versions

`.github/workflows/publish.yml` publishes on a pushed `v<version>` tag. It checks
that the tagged commit belongs to `main`, all three library versions match the tag,
and internal dependency versions match. The workflow runs the standard gate and
documentation checks, then a publication dry-run before authenticating.

Authentication uses [crates.io Trusted Publishing](https://crates.io/docs/trusted-publishing).
GitHub supplies an OIDC identity; the official action exchanges it for a temporary
publishing token and revokes that token when the job ends. No permanent API key or
GitHub repository secret is required. The local `.env` file is not used by CI.

### One-time setup

For each of `igs-iec104-codec`, `igs-iec104-link` and `igs-iec104`, open the crate
settings on crates.io and add a GitHub trusted publisher with these values:

| Field | Value |
| --- | --- |
| Repository owner | `aklira` |
| Repository name | `igs-iec104` |
| Workflow filename | `publish.yml` |
| GitHub environment | `crates-io` |

In GitHub repository Settings > Environments, create `crates-io` and allow the
release tags (`v*`). Optional required reviewers can add a confirmation before
publishing. Keep the same environment name in GitHub, crates.io and the workflow.
Merge the workflow into `main` before creating a release tag.

### Release procedure

1. Update all three library package versions together, their internal dependency
   requirements, Cargo.lock and the changelog. Version 0.1.0 already exists.
2. Run the standard gate and merge the release commit into `main`.
3. Create and push the corresponding tag, for example `v0.1.1`, on that commit.
4. Check the Publish to crates.io workflow and the three registry pages.

The workflow explicitly selects the three libraries; it does not publish `xtask`
or the bench probe. Cargo publishes in dependency order. If an upload fails partway
through, rerun the workflow: the planner skips versions already on crates.io.
Registry errors other than HTTP 404 and yanked versions stop the release.
Publishing a tag is deliberate release authorization; it does not waive the
remaining 1.0 review gates below. No new version or tag is created automatically.

## Remaining 1.0 gates

The following remain pending and are not certified by the 0.1.0 publication:

- C7: the maintainer's 24-hour fuzzing campaign.
- S5: load and redundancy switchover tests under load.
- Coverage thresholds: at least 85% codec and 75% link/server.
- TLS interoperability scenarios with the reference peer.
- X3: independent similarity check, legal review and final maintainer approval.

See the implementation plan and the C7, S5 and X1 AI logs for the recorded gaps.
