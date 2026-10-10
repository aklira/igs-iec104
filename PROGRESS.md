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
| C4 ASDU bodies | Done, committed | log `docs/ai-log/C4.md`, decision D-010 in `PROVENANCE.md`, open questions Q-001 and Q-002 in `QUESTIONS.md` |
| C5 APCI framing | Done, committed | log `docs/ai-log/C5.md`, decision D-011 in `PROVENANCE.md` |
| C6 Property tests | Done, committed | log `docs/ai-log/C6.md`; `proptest` added as a dev-dependency (approved) |
| C7 Fuzzing ⛔ | Partly done: harness committed, 10 min per target without crash; 24 h campaign pending (maintainer gate) | log `docs/ai-log/C7.md`; `libfuzzer-sys` and the `NCSA` licence approved |
| L1 Parameters | Done, committed | log `docs/ai-log/L1.md`, decision D-012 in `PROVENANCE.md` (recommendations are warnings) |
| L2 State machine core | Done, committed `c543252`: both roles follow figures 17 and 18 (figure 18 read from the PDF page 51); window, wrap-around and N(R) checks tested with a fake clock | log `docs/ai-log/L2.md`; open questions Q-007 and Q-008 in `QUESTIONS.md` |
| L3 Timers and tests | Done, not committed: t1, t2 and t3 with fake-clock tests; `Session::new` takes the establishment instant and `next_deadline()` replaces a wake-up action | log `docs/ai-log/L3.md`; open questions Q-009 to Q-013 in `QUESTIONS.md` (figures 12 to 14 missing from the prepared text) |
| T1 Transport | Done, not committed: async driver over `tokio::net::TcpStream` with t0 connect timeout, clean shutdown and typed errors; generic `Transport` bound for TLS; `Delivery::Transfer` reports the state of the data transfer | log `docs/ai-log/T1.md`; tests: 12 in `igs-iec104` (in memory on a paused clock, and TCP loopback) |
| L4 Interop APCI ⛔ | Done for the accepted scope, committed `40b95a9`: probe (`bench/probe`) and 8 scenarios pass in both directions; the wrap of N(S) is an accepted known gap (expected failure). Fault injection moved to L5 | log `docs/ai-log/L4.md`; bench tests `tests/test_l4_interop.py` |
| L5 Fault injection interop | Not started: relay in the probe, t1 expiry, dropped and delayed frames | — |

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
| `NCSA` added to the licence allow list in `deny.toml` | `libfuzzer-sys` embeds compiler-rt, licensed NCSA. Only the fuzzing tool uses it; it is never shipped. Approved by the maintainer; see `docs/ai-log/C7.md` |

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

- APCI and framing: the `apci` module (`Apdu`, `FrameDecoder`). The control
  bit positions come from the figure text in the raw pages of §5 (see C5), so a
  maintainer should check figures 6 to 8 against the PDF.
- C7 (fuzzing): the harness is committed and passes its ten-minute check on both
  targets. The 24-hour campaign is a maintainer gate (see `docs/ai-log/C7.md` for the
  commands).
- L1 (parameters) is done: `LinkConfig` in `igs-iec104-link`.
- L2 (state machine core) is committed. Q-003, Q-005 and Q-006 are decided (D-013: the
  connection closes). Q-007 and Q-008 stay open.
- T1 (transport) is committed (`105d52d`). L4 (interop APCI) is done for the accepted scope,
  committed (`40b95a9`); the maintainer moved fault injection to L5, the next bench task.
- L3 (timers) is committed (`84d1546`). Its questions Q-009 to Q-013
  rest on figures 12 to 14, which are missing from the prepared text. The figures are
  for a maintainer to read before the reaction is confirmed.
- The generated profile is ready. `object_size_bits(125) == None` is handled by
  the `FileSegment` value (length from LOS, see C4).
