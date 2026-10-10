# Progress

Running log of the implementation (see `IMPLEMENTATION.md` for the plan and
`docs/ai-log/` per task). Nothing is ever guessed: an open standard question
goes to `docs/ai-log/QUESTIONS.md`, never into code.

## Task status

| Task | Status | Where |
| --- | --- | --- |
| F1 Workspace skeleton | Done, committed | `9f562ac`, log `docs/ai-log/F1.md` |
| F2 Continuous integration | Done, committed | `8f179a8`, log `docs/ai-log/F2.md` |
| F3 Code generation | Done, committed `bd7afd5` | log `docs/ai-log/F3.md` |
| F5 AI log and questions | Done, committed | `091cf52`, log `docs/ai-log/F5.md` |
| F4 Interop bench skeleton | Done, committed | log `docs/ai-log/F4.md` |
| C1 Primitive formats | Done, committed | log `docs/ai-log/C1.md`, decisions D-006 and D-007 in `PROVENANCE.md` |
| C2 Information elements | Done, committed | log `docs/ai-log/C2.md`, decisions D-008 and D-009 in `PROVENANCE.md` |
| C3 ASDU header | Done, committed | log `docs/ai-log/C3.md`, typed errors in `error.rs` |

## Decisions taken (beyond the plan)

| Decision | Reason |
| --- | --- |
| `rust-version = "1.85"` in the workspace | the plan asks for the field but sets no floor; 1.85 is the first stable with `[lints]` tables in `Cargo.toml` |
| `serde` + `serde_yaml` as **xtask**-only dependencies | F3 needs a YAML reader and no dependency was listed; approved verbally by the maintainer. Unpublished crate only, passes `cargo deny`. |
| `Cargo.lock` committed | reproducible CI builds |
| Generated code lives at `crates/igs-iec104-codec/src/generated/` (`mod.rs` + `profile.rs`), is wired as `pub mod generated;` and committed | the plan says "committed: CI cannot regenerate it" because `docs/spec/` is a local symlink absent in CI |
| `codegen` formats its own output with `rustfmt` and `--check` compares byte-exact | keeps the stale check deterministic and keeps the gate's `cargo fmt --check` green |
| Generated accessors are exhaustive `match` on a `#[repr(u8)]` enum, no indexing, no unchecked arithmetic | the denied lints of the codec crate (`unwrap_used`, `indexing_slicing`, `arithmetic_side_effects`...) must stay silent on generated code |
| Rust mnemonics (`M_SP_NA_1`) are enum variants with `#[allow(non_camel_case_types)]` | data keeps its canonical spelling |
| Method name `in_profile` instead of the planned `in_104_profile` | Rust naming; semantics identical |
| `sq_allowed` exposed as a bit mask, `object_size_bits` as `Option<u16>` | type ID 125 (Segment) has a null object size in the data: variable-length file transfer |
| `llvm-tools-preview` added to `rust-toolchain.toml` | `cargo llvm-cov` needs it on the pinned toolchain |
| `.gitignore` left untouched | an uncommitted edit there would start ignoring `IMPLEMENTATION.md` which is tracked; contradicts the plan; needs a maintainer call |

## Open questions for maintainers

Not standard ambiguities (none encountered in phase 0 so far), but project
decisions:

1. ~~F4 gate: which reference peer image, which licence?~~ Resolved: FledgePower
   image `ghcr.io/aklira/fledgepower/fledge:v1.2.4`, black-box use (see
   `docs/ai-log/F4.md`).
2. Is the pending `.gitignore` change (ignoring `AGENTS.md`,
   `IMPLEMENTATION.md`, `opencode.json`) wanted? It contradicts the plan of
   record being tracked.
3. Coverage thresholds are expected at X3; F2's coverage step is report-only
   until then.

## Notes for the next tasks

- The interop bench runs with `python3 -m pytest` in `bench/` (Docker or
  Podman). It pins FledgePower v1.2.4 by digest and starts each peer with
  `bench/peer/bootstrap.sh` (one service per container).

- Open point for C4: the header returns `DecodeError` (typed), while the C1 and C2
  element types return `Option`. Pick one error path for the ASDU body, either
  by mapping the element results or by moving the elements to `Result`.
- The generated profile is ready for C4. `object_size_bits(125) == None` must
  be handled when C4 decodes. C4 (ASDU bodies) is the next codec task: C2 and
  C3 are done. It reads the element types in `elements`; the all-zero range
  time of F_SC_NB_1 is `RangeTime::Unbounded`.
