# X3-ci - Automatic crates.io publication

## Prompt (translated)

Would it be useful to add an automatic crates.io publishing workflow, and would
the API key need to be stored in GitHub repository secrets?

## Implementation

- Work on codex/cratesio-publishing; keep main unchanged.
- Add publish.yml, triggered by v-prefixed tags in aklira/igs-iec104.
- Require a commit included in main, matching versions for all three libraries,
  and matching internal dependency versions before publication.
- Run the standard gate, documentation and a locked publication dry-run.
- Authenticate through the official crates-io-auth-action v1.0.6, pinned to its
  verified commit. Limit OIDC permissions to the publishing job and expose the
  temporary token only to its cargo publish step.
- Select the three libraries explicitly. Skip existing versions on retries;
  abort on registry failures or yanked versions. Never publish xtask or the probe.
- Add eight release-planning regression tests and run them in CI. Add actionlint
  to CI and document the one-time trusted-publisher and environment setup.
- Do not read or transfer the local API key. No repository secret is required.

## Validation and activation

All checks passed: actionlint 1.7.12 on both workflows, eight release-planning
tests, and the standard gate (427 Rust tests, none failed). The locked publication
command passed in dry-run mode; the live registry planner correctly skipped all
three existing 0.1.0 versions. Documentation also builds with warnings denied.

Live OIDC exchange cannot be tested locally: each crate must authorize owner aklira, repository igs-iec104,
workflow publish.yml and environment crates-io in its crates.io settings. The
workflow must then be merged and a future version tag deliberately pushed.
No external settings, pushes, tags or registry uploads are part of this change.

Sources: https://crates.io/docs/trusted-publishing and
https://github.com/rust-lang/crates-io-auth-action.
