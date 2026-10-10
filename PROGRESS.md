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
| L3 Timers and tests | Done, committed `84d1546`: t1, t2 and t3 with fake-clock tests; `Session::new` takes the establishment instant and `next_deadline()` replaces a wake-up action | log `docs/ai-log/L3.md`; open questions Q-009 to Q-013 in `QUESTIONS.md` (figures 12 to 14 missing from the prepared text) |
| T1 Transport | Done, committed `105d52d`: async driver over `tokio::net::TcpStream` with t0 connect timeout, clean shutdown and typed errors; generic `Transport` bound for TLS; `Delivery::Transfer` reports the state of the data transfer | log `docs/ai-log/T1.md`; tests: 12 in `igs-iec104` (in memory on a paused clock, and TCP loopback) |
| L4 Interop APCI ⛔ | Done for the accepted scope, committed `40b95a9`: probe (`bench/probe`) and 8 scenarios pass in both directions; the wrap of N(S) is an accepted known gap (expected failure). Fault injection moved to L5 | log `docs/ai-log/L4.md`; bench tests `tests/test_l4_interop.py` |
| L5 Fault injection interop | Done: APCI relay in the probe; nine scenarios pass in both directions (t1 expiry, late and dropped and duplicated frames) | log `docs/ai-log/L5.md`; bench tests `tests/test_l5_faults.py` |
| T2 Client API | Done, committed `0056c04`: `Client` with reconnect policy, the procedures of §7 and an event stream; nine integration tests on loopback | log `docs/ai-log/T2.md`; tests `crates/igs-iec104/tests/client.rs` |
| T3 `igs104-client` binary | Done, committed `c1bbba0`: clap CLI with eleven subcommands. Its manual check was wrong for negative confirmations; corrected in T4 | log `docs/ai-log/T3.md` (correction section); date and judgement unit tests in the binary |
| T4 Client interop matrix ⛔ | Done, gate closed by the maintainer (option 1: the matrix is the exit of phase 3, known gaps recorded). 33 rows on the reference server; 5 are positive exchanges, 27 check the peer's refusal. Found and fixed the negative-confirmation defect in T3 | log `docs/ai-log/T4.md`; bench tests `bench/tests/test_t4_client_matrix.py` |
| S1 Process image | Done, committed `aeb45e1`: `ProcessImage` in `igs-iec104` with ten monitoring types (time-tagged forms where they exist), change detection on value and quality, a bounded spontaneous queue with drop-oldest and a drop counter; unit and model-based property tests | log `docs/ai-log/S1.md`; tests in `crates/igs-iec104/src/process_image/tests.rs` |
| S2 Server | Done, committed `f3377a3`: `Server` for controlled stations in `igs-iec104` with a sans-I/O `Responder` for the procedures of §7.2 and §7.5 to §7.10, end of initialization, select-before-operate with time-outs, negative confirmations and mirrors with causes 44 to 47; spontaneous events to the started connections; `ServerHandle` to publish points; the CP56 conversion moved to `clock`. Unit tests (22 responder, 2 image groups) and 17 integration tests with the client | log `docs/ai-log/S2.md`; questions Q-014 and Q-015 in `QUESTIONS.md`; tests `crates/igs-iec104/tests/server.rs` |
| S3 Redundancy groups | Done, committed `72b2c35`: the server is a redundancy group (one started connection, the image as the event buffer, supersede on start, the unacknowledged ASDUs carried to the connection that takes over); link keeps the sent ASDUs and takes `Supersede`; `RedundantClient` for the controlling station (first start, interrogation after each start, switchover on loss and by hand). Figures 32 to 37 read from the PDF. Tests: 5 server switchover tests with a fake clock, 4 redundancy tests over loopback, 5 link tests | log `docs/ai-log/S3.md`; questions Q-016 to Q-018 in `QUESTIONS.md`; tests `crates/igs-iec104/src/server/tests.rs`, `crates/igs-iec104/src/redundancy/tests.rs` |
| S4 `igs104-server` binary | Partly done, committed `9f0f0c2` with the built-in points only: the binary serves a built-in demo set with simulated changes, accepts commands at its command points, and is documented (`docs/igs104-server.md`). The points from a TOML file wait for approval of the `toml` crate, which is not in the plan's list of dependencies | log `docs/ai-log/S4.md`; tests `crates/igs-iec104/tests/server_binary.rs` and the binary's unit tests |
| X2 Documentation and examples | Done, committed in `15a06fe` and in the commit that adds the `tls` example: `#![deny(missing_docs)]` in the three library crates; `client`, `server` and `redundancy` examples; README quick start and MSRV policy; `CHANGELOG.md`; CI documentation step and MSRV job (Rust 1.85). The `tls` example was added with the X1 transport increment | log `docs/ai-log/X2.md`; box in `IMPLEMENTATION.md` §6 ticked in the commit that adds the TLS example |
| X1 TLS (IEC 62351-3) | Partly done, committed `eb8a0a8` (profile, events, settings, rustls backend) and `c36f402` (transport, client and server options): `tokio-rustls` (approved), `TlsBackend`, `Secure`, client and server TLS options, five loopback tests, the `tls` example. Not yet: OpenSSL backend (headers or vendored: a decision), the TLS bench scenarios, the events and intervals listed in `docs/ai-log/X1.md`. Gaps: Q-025; new question Q-027 | log `docs/ai-log/X1.md`; questions Q-019 to Q-027 in `QUESTIONS.md`; tests `crates/igs-iec104/src/tls/`, `crates/igs-iec104/tests/tls.rs`; box in `IMPLEMENTATION.md` §6 left open |

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
| MSRV job runs `cargo +1.85`, not the plain `cargo` | `rust-toolchain.toml` pins `stable`, which would override an installed 1.85 and test the wrong compiler silently |
| `rustls` 0.23 with the `ring` provider for the TLS backend | the plan names rustls; `ring` needs only a C compiler, its licence is Apache-2.0 AND ISC, and `cargo deny` passes (one `windows-sys` duplicate warning) |

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
- T4 (client matrix) is done; the maintainer chose option 1 at the gate. The positive command path, counter
  interrogation and read against the reference server are known gaps (see `docs/ai-log/T4.md`). A bench
  task for a control path would give positive commands before S5. The reference configuration gained a
  clock synchronization (`time_sync`) and one command point per command type.
- T1 (transport) is committed (`105d52d`). L4 (interop APCI) is done for the accepted scope,
  committed (`40b95a9`); the maintainer moved fault injection to L5, the next bench task.
- L3 (timers) is committed (`84d1546`). Its questions Q-009 to Q-013
  rest on figures 12 to 14, which are missing from the prepared text. The figures are
  for a maintainer to read before the reaction is confirmed.
- The generated profile is ready. `object_size_bits(125) == None` is handled by
  the `FileSegment` value (length from LOS, see C4).
