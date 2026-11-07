# X3-sync - Reconcile the publication branch

## Prompt (translated and summarized)

The maintainer reported that pulling origin/main fails because the local and
remote branches have diverged after the publication changes.

## Findings and resolution

- Local main contained three publication commits absent from origin/main.
- origin/main contained 2d8a260, a dependency lockfile update, absent locally.
- Both histories started from 0d1500d. The worktree was initially clean.
- Keep a local recovery branch: codex/before-main-reconcile-20261107.
- Merge origin/main into main. Preserve the publication commits, including
  c586279, which is referenced by the published packages.
- Resolve the only conflict, in Cargo.lock, by retaining the remote lockfile:
  Tokio 1.54.1, rustls 0.23.46 and rustls-webpki 0.103.16.
- Do not change pull configuration, rewrite commits, push or republish crates.

## Validation

The standard gate passed after resolving the lockfile:
- cargo fmt --all --check
- cargo clippy --workspace --all-targets --all-features -- -D warnings
- cargo test --workspace --all-features: 427 passed, none failed
- cargo deny check: advisories, bans, licenses and sources pass

The merge incorporates origin/main, preserving both histories. Only the task
record and plan were added after validation; runtime source was not changed.
