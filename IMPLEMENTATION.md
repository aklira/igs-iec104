# Implementation plan

This plan drives the implementation of `igs-iec104` up to a ready-to-use 1.0 release. It is written
for an AI coding agent (typically a local LLM) working task by task, under human supervision.
Humans use it to track progress and to review each step.

Scope of 1.0: APCI/ASDU codec for every type ID of the 104 profile, CS104 client, multi-client CS104
server, redundancy groups, TLS per IEC 62351-3. File transfer procedures come in 1.1, CS101 serial
in 2.0 (separate crate).

---

## 1. Rules for the agent (read before every task)

1. **Clean room.** Never open, quote, reconstruct or describe the source code of lib60870 (C, .NET),
   its headers, its guides, or the crates `iec60870-5` and `lib60870` bindings. Reference
   implementations are only ever run as separate processes in the interop bench.
2. **No standard text in this repository.** IEC standards are licensed. Never paste text from
   `docs/spec/` into code, comments, docs, tests or commit messages. Cite the clause
   (`// IEC 60870-5-104 §5.1`) and write in your own words. Facts such as type IDs, bit positions,
   sizes and cause lists may be used as data.
3. **English only** for everything committed: code, comments, docs, commit messages.
4. **Every protocol behaviour needs a source.** Take values from the data files of section 3, not
   from memory. If the standard is silent or ambiguous, do not guess: add an entry to
   `docs/ai-log/QUESTIONS.md` (task ID, question, clauses read) and move on to another task.
   Resolved questions become entries in `PROVENANCE.md` and permanent tests.
5. **One task at a time.** Read only the inputs listed for the task. Keep each change small (aim for
   under 400 changed lines per commit). Do not start a task whose dependencies are not checked.
6. **Done means green.** A task is done only when all its done criteria pass, plus the standard gate:
   ```
   cargo fmt --all --check
   cargo clippy --workspace --all-targets --all-features -- -D warnings
   cargo test --workspace --all-features
   cargo deny check
   ```
7. **Commit per task**, never push. Message: `<crate or area>: <what>` then a blank line and
   `Task: <ID>`. Tick the task's box in section 6 in the same commit.
8. **Safety.** `#![forbid(unsafe_code)]` in `igs-iec104-codec` and `igs-iec104-link`. No `unwrap`,
   `expect`, indexing that can panic, or unchecked arithmetic on data from the network: return
   `Result`. Bound every length read from the wire.
9. **Dependencies** must be MIT, Apache-2.0, BSD or ISC and pass `cargo deny`. Ask before adding
   any dependency not listed in this plan.
10. **Human gates** (marked ⛔ in section 6) require explicit approval by a maintainer. Stop and ask.
11. **Log prompts.** Archive the prompt and a short summary of each task in
    `docs/ai-log/<task-id>.md` (no standard text).

---

## 2. Architecture (summary)

Cargo workspace:

| Crate | Role | I/O | async |
| --- | --- | --- | --- |
| `igs-iec104-codec` | APCI framing, ASDU encode/decode, information elements, time formats | none | no |
| `igs-iec104-link` | APCI session state machine: sequence numbers, k/w, t1–t3, STARTDT/STOPDT/TESTFR | none | no |
| `igs-iec104` | Public API: `Client`, `Server`, redundancy, process image, TLS, binaries `igs104-client` and `igs104-server` | TCP/TLS (tokio) | yes |
| `xtask` | Code generation and maintenance commands (not published) | files | no |

Design rules:

- **Sans-I/O session**: the link state machine takes events (bytes received, current instant, user
  requests) and returns actions (frames to send, ASDUs to deliver, next wake-up time, close).
- **Strong types**: one type ID = one enum variant; common address, IOA, COT and qualifiers are
  newtypes; an invalid frame cannot be built.
- **Two-level validation**: structural decoding (sizes, ranges) in the codec; profile validation
  (type ID allowed in 104, COT allowed for that type ID) as a separate step that the server and
  client call.
- **Events** via tokio channels; incoming commands handled through traits.

---

## 3. Sources of truth

`docs/spec/` is a local symlink to the private standards repository; it is ignored by git and not
available in CI. Read `docs/spec/<standard>/INDEX.md` first, then only the section you need.

**Data files (use these for any value that becomes code):**

| File | Content |
| --- | --- |
| `docs/spec/104/data/profile_104.yaml` | ASDU header sizes; 67 type IDs with `in_104_profile`, information elements and sizes, `object_size_bits`, `time_tag`, `sq_allowed`, `cot_104` (allowed causes), `cot_not_permitted`, `cot_decision` |
| `docs/spec/5-4/data/formats.yaml` | Bit numbering, transmission order, UI, I, F, F16, R32, BS, CP56Time2a, CP24Time2a, CP16Time2a |
| `docs/spec/101/data/element_sizes.yaml` | Size and definition clause of each information element |
| `PROVENANCE.md` | Decisions D-001 to D-005 on cause-of-transmission discrepancies |

**Sections by topic** (paths under `docs/spec/`):

| Topic | Sections |
| --- | --- |
| APCI, I/S/U formats, APDU length | `104/sections/005_5_*`, `004_4_*` |
| Sequence numbers, acknowledgements, t1/t2 | `104/sections/006_5.1_*` |
| TESTFR, t3 | `104/sections/007_5.2_*` |
| STARTDT/STOPDT | `104/sections/008_5.3_*` |
| Port number | `104/sections/009_5.4_*` |
| k, w | `104/sections/010_5.5_*` |
| Timer and parameter defaults (t0–t3, k, w) | `104/sections/040_9.6_*` |
| ASDU header fields, sizes, byte order | `104/sections/039_9.5_*`, `101/sections/024_7.1_*`, `101/sections/025_7.2_*` (7.2.1 to 7.2.5) |
| Information elements | `101/sections/025_7.2_*` (7.2.6) |
| ASDU definitions | `101/sections/026_7.3_*`, `104/sections/024_8_*` to `033_8.9_*` |
| Application procedures (init, GI, commands, clock sync, counters, parameters, test) | `104/sections/012_7_*` to `023_7.11_*`, `101/sections/027_7.4_*` |
| Redundancy | `104/sections/041_10_*` to `048_10.7_*` |
| TLS profile | `62351-3/sections/019_6_*` to `039_8.8_*`, conformance `041_10_*` to `047_10.6_*` |
| Security events | `62351-3/sections/048_Annex_A_security_events.md` |

Figures (sequence diagrams, state diagrams, ASDU layouts) are missing from the prepared text
(`[FIGURE ... MISSING]`). When a task depends on one, flag it in `QUESTIONS.md`; a maintainer
will consult the PDF. **The PDF is the authority** when text and data disagree.

---

## 4. Phases and tasks

Each task lists: **Depends**, **Read**, **Produce**, **Done when**.

### Phase 0 — Foundations

**F1 — Workspace skeleton**
- Depends: none.
- Produce: root `Cargo.toml` (workspace, edition 2021, `rust-version`), `rust-toolchain.toml`
  (stable), crates `igs-iec104-codec`, `igs-iec104-link`, `igs-iec104`, `xtask`; workspace lints
  (`unsafe_code = "forbid"` for codec and link, `clippy::unwrap_used`, `clippy::expect_used`,
  `clippy::indexing_slicing`, `clippy::arithmetic_side_effects` denied in codec and link);
  `deny.toml` allowing MIT, Apache-2.0, BSD-2/3, ISC, Unicode-3.0; Apache-2.0 SPDX header in each
  source file.
- Done when: standard gate passes on the empty crates.

**F2 — Continuous integration**
- Depends: F1.
- Produce: `.github/workflows/ci.yml` running the standard gate, `cargo llvm-cov` (report only),
  on Linux; the workflow must not need `docs/spec/`.
- Done when: workflow is valid (`actionlint` if available) and the gate passes locally.

**F3 — Code generation from the profile**
- Depends: F1.
- Read: `profile_104.yaml`, `formats.yaml` (structure only).
- Produce: `cargo xtask codegen` reading the two YAML files and writing
  `crates/igs-iec104-codec/src/generated/profile.rs`: a `TypeId` enum (all 67 type IDs, with
  `in_104_profile()`), per type ID: mnemonic, `sq_allowed`, object size in bits, time tag, allowed
  and not-permitted COT sets as `const` arrays; ASDU header sizes as constants. Generated file
  starts with a "generated by `cargo xtask codegen`, do not edit" banner and cites its sources by
  clause. The generated file is committed (CI cannot regenerate it). Add
  `cargo xtask codegen --check` that fails if the committed file is stale.
- Done when: generated file compiles; a unit test asserts 67 type IDs, 54 in profile, and that
  TI 103 does not allow COT 3 (D-004).

**F4 — Interop bench skeleton** ⛔
- Depends: F1.
- Produce: `bench/` with `pyproject.toml` (pytest), `docker-compose.yml` running a reference peer
  as a separate container, a capture helper (`tcpdump`) and an assertion helper based on
  `tshark -T json`; README explaining how to run it.
- Gate: a maintainer confirms the reference peer image and its licence (black-box use only).
- Done when: one scenario connects to the reference server, sends STARTDT and a general
  interrogation with an existing tool, and the capture decodes in `tshark` without errors.

**F5 — AI log and questions**
- Depends: none.
- Produce: `docs/ai-log/README.md` (convention), `docs/ai-log/QUESTIONS.md` (empty table:
  ID, task, question, clauses, status).
- Done when: files exist and are linked from `README.md`.

### Phase 1 — Codec (`igs-iec104-codec`)

**C1 — Primitive formats**
- Depends: F3.
- Read: `formats.yaml`.
- Produce: module `formats` with bit-level reader/writer over byte slices (bit numbering of 4.3,
  little-endian octet order per 104 §9.5); types for UI(n), I16, F16 (normalized, with conversion
  to/from `f32`), R32 (IEEE 754, `f32::from_bits`), BS(n), `Cp56Time2a`, `Cp24Time2a`,
  `Cp16Time2a` with range-checked constructors (milliseconds 0..59 999, minutes 0..59, hours 0..23,
  day of month 1..31, day of week 0..7 with 0 = unused, month 1..12, year 0..99, IV, SU, RES1).
- Done when: unit tests cover min/max values of every field, out-of-range rejection, and at least
  one hand-written byte vector per format derived from the bit layout in `formats.yaml`.

**C2 — Information elements**
- Depends: C1.
- Read: `101/sections/025_7.2_*` (7.2.6), `element_sizes.yaml`.
- Produce: module `elements` with one type per element used by in-profile type IDs (SIQ, DIQ, QDS,
  QDP, VTI, NVA, SVA, short float, BCR, SEP, SPE, OCI, BSI, SCO, DCO, RCO, SCD, QOI, QCC, QPM, QPA,
  QRP, QOS, COI, TSC, and the file-transfer elements NOF, NOS, LOF, LOS, CHS, SOF, FRQ, SRQ,
  SCQ, LSQ, AFQ, segment; CP16Time2a and the QueryLog range times reuse the C1 time types), each
  with `encode`/`decode` and named flags (no raw bit masks in the public API). Each type cites its
  clause. The authoritative list is the set of `information_elements` of in-profile type IDs in
  `profile_104.yaml`; elements used only by type IDs outside the profile (e.g. FBP) are not needed.
- Done when: every element round-trips; sizes match `element_sizes.yaml` (test reads the
  generated constants, not the YAML).

**C3 — ASDU header**
- Depends: C1, F3.
- Read: `101/sections/025_7.2_*` (7.2.1 to 7.2.5), `104/sections/039_9.5_*`.
- Produce: `TypeId` (generated), variable structure qualifier (SQ, number of objects), cause of
  transmission (cause 0..63, P/N, test bit, originator address), common address (2 octets),
  information object address (3 octets).
- Done when: round-trip tests; decoding rejects truncated input with a typed error.

**C4 — ASDU bodies**
- Depends: C2, C3.
- Read: `profile_104.yaml`, `101/sections/026_7.3_*`, `104/sections/024_8_*` to `033_8.9_*`.
- Produce: `Asdu` with one variant per in-profile type ID, supporting SQ = 0 and SQ = 1 where
  `sq_allowed` permits; decode checks object count against remaining length and the 253-octet APDU
  limit; separate `validate_profile(&Asdu, direction)` returning which rule failed (type ID not in
  profile, COT not in `cot_104`, COT not permitted). Type IDs outside the 104 profile decode to an
  `UnsupportedTypeId` error carrying the raw bytes.
- Done when: one test vector per in-profile type ID (hand-built from the generated sizes), and
  tests for D-001 to D-005 via `validate_profile`.

**C5 — APCI framing**
- Depends: C4.
- Read: `104/sections/005_5_*`, `004_4_*`.
- Produce: `Apdu` (I-format with N(S), N(R) and ASDU; S-format with N(R); U-format with
  STARTDT/STOPDT/TESTFR act/con), and an incremental `FrameDecoder` that accepts arbitrary byte
  chunks, returns complete APDUs, reports framing errors, and never buffers more than one maximum
  APDU.
- Done when: tests feed frames split at every byte boundary and concatenated frames.

**C6 — Property tests**
- Depends: C5.
- Produce: `proptest` strategies generating every valid ASDU and APDU; `decode(encode(x)) == x`;
  decoding random bytes never panics.
- Done when: 10 000 cases per property pass.

**C7 — Fuzzing** ⛔
- Depends: C5.
- Produce: `fuzz/` (cargo-fuzz) targets `apdu_decoder` and `asdu_decode`, with a seed corpus from
  the C4 vectors.
- Gate: a maintainer runs a 24 h campaign with no crash (phase 1 exit criterion).
- Done when: each target runs 10 minutes locally without a crash.

### Phase 2 — Link session (`igs-iec104-link`)

**L1 — Parameters**
- Depends: C5.
- Read: `104/sections/040_9.6_*`, `010_5.5_*`.
- Produce: `LinkConfig` (t0, t1, t2, t3, k, w) with defaults taken from 104 §9.6 and validation
  of ranges and relations stated in the standard; role (controlling / controlled).
- Done when: tests for defaults and invalid configurations.

**L2 — State machine core**
- Depends: L1.
- Read: `104/sections/006_5.1_*`, `008_5.3_*`.
- Produce: `Session` (sans-I/O) with `handle(event, now) -> Vec<Action>`; events: bytes received,
  tick, user send ASDU, user STARTDT/STOPDT; actions: send APDU, deliver ASDU, schedule wake-up,
  close with reason. Sequence numbers modulo 32768, send window k, acknowledgement after w
  received I-frames, STARTDT/STOPDT states.
- Done when: deterministic tests with a fake clock for: normal exchange, window full, wrap-around
  at 32768, and the handling of a received N(R) that acknowledges frames never sent, as specified
  in §5.1 (if the prepared text does not state it, record a question in `QUESTIONS.md`).

**L3 — Timers and tests**
- Depends: L2.
- Read: `104/sections/006_5.1_*`, `007_5.2_*`.
- Produce: t1 (unacknowledged frame or test frame), t2 (delayed acknowledgement), t3 (idle →
  TESTFR act), handling of TESTFR act/con.
- Done when: fake-clock tests for each timer expiry and reset rule; scenarios flagged as relying
  on a missing figure are listed in `QUESTIONS.md`.

**L4 — Interop APCI** ⛔
- Depends: L3, F4, T1.
- Produce: bench scenarios: STARTDT, TESTFR, t2 and w, the window of k, and the wrap-around.
  Fault injection moved to L5 by the maintainer.
- Gate: maintainer reviews the scenario list against the phase 2 exit criterion.
- Done when: the accepted scenarios pass in both directions. The wrap of N(S) in the
  controlled direction (the reference client closes after about 60 s) is an accepted known
  gap, recorded as an expected failure in the bench.

**L5 — Fault injection interop**
- Depends: L4.
- Produce: an APCI-aware relay in the probe, which drops, delays or duplicates frames on
  request; scenarios for the expiry of t1 with a silent peer (both directions), a dropped frame
  (disturbed sequence, figure 11, Q-005), a delayed acknowledgement and a duplicated frame.
- Done when: the scenarios pass in both directions, with the results in `docs/ai-log/L5.md`.

### Phase 3 — Client (`igs-iec104`)

**T1 — Transport**
- Depends: L3.
- Produce: async driver running a `Session` over `tokio::net::TcpStream` (t0 connect timeout,
  default port 2404 from 104 §5.4), clean shutdown, error types; generic over a stream trait so
  TLS plugs in later.
- Done when: tests against an in-process loopback server using `tokio::time::pause`.

**T2 — Client API**
- Depends: T1.
- Read: `104/sections/013_7.1_*` to `022_7.10_*`.
- Produce: `Client` with connect/reconnect policy, start/stop data transfer, general and counter
  interrogation, read, clock synchronization (never COT 3, D-004), single/double/regulating step
  and set-point commands with select/execute and time-tagged variants, test command; an event
  stream of received ASDUs and connection state; bounded channels with documented back-pressure.
- Done when: integration tests against the loopback server for each procedure.

**T3 — `igs104-client` binary**
- Depends: T2.
- Produce: CLI (clap) to connect, interrogate, send commands, print decoded ASDUs.
- Done when: `--help` documented; manual check against the bench server recorded in the ai-log.

**T4 — Client interop matrix** ⛔
- Depends: T2, F4.
- Produce: bench scenarios with the client against the reference server for every procedure of T2.
- Gate: phase 3 exit criterion reviewed by a maintainer.
- Done when: 100 % of the client matrix passes.

### Phase 4 — Server and redundancy

**S1 — Process image**
- Depends: C4.
- Produce: typed point database (by common address and IOA) with change detection and an event
  queue for spontaneous transmission (COT 3), bounded, with an overflow policy.
- Done when: unit and property tests.

**S2 — Server**
- Depends: S1, T1.
- Read: `104/sections/013_7.1_*` to `022_7.10_*`, `PROVENANCE.md`.
- Produce: multi-client `Server`: end of initialization, general/group interrogation (activation
  confirmation, interrogated data, activation termination), counter interrogation, read, clock
  synchronization, command handling through a trait with select-before-operate and timeouts,
  negative confirmations and mirrored ASDUs with COT 44–47 for unknown type, cause, common address
  or IOA, profile validation on every received ASDU.
- Done when: integration tests per procedure with the `Client`.

**S3 — Redundancy groups**
- Depends: S2.
- Read: `104/sections/041_10_*` to `048_10.7_*` (state diagrams are missing: ask for the PDF
  figures through `QUESTIONS.md` before implementing).
- Produce: groups of connections where only the connection that received STARTDT carries data;
  switchover on STARTDT on another connection; retransmission of unacknowledged I-frames after
  switchover as specified in §10.5–10.6; client-side redundancy for the controlling station.
- Done when: fake-clock tests for switchover with and without in-flight frames, no data loss.

**S4 — `igs104-server` binary**
- Depends: S2.
- Produce: configurable demo server (points from a TOML file, simulated value changes).
- Done when: documented; usable as the bench device under test.

**S5 — Server interop, load and switchover** ⛔
- Depends: S3, S4, F4.
- Produce: bench scenarios with the reference client; load test with 50 clients and 10 000 points;
  switchover under load.
- Gate: phase 4 exit criterion reviewed by a maintainer.
- Done when: interop matrix passes, load test meets agreed latency, switchover loses no data.

### Phase 5 — TLS, documentation, release

**X1 — TLS**
- Depends: T1.
- Read: `62351-3/sections/019_6_*` to `039_8.8_*`, conformance `044_10.3_*` to `047_10.6_*`,
  `048_Annex_A_security_events.md`.
- Produce: `TlsBackend` trait; `rustls` backend (default feature) and `openssl` backend (optional
  feature) for renegotiation; configuration restricted to the TLS versions, cipher suites and
  extensions marked mandatory or optional in the conformance tables; certificate validation;
  security events emitted with the mnemonics of Annex A.
- Done when: loopback TLS tests for client and server, rejection of a disallowed cipher suite, a
  test per security event that can be triggered locally; TLS scenarios added to the bench.

**X2 — Documentation and examples**
- Depends: T2, S2.
- Produce: rustdoc on every public item (`#![deny(missing_docs)]` in `igs-iec104`), `examples/`
  (client, server, redundancy, TLS), README quick start, `CHANGELOG.md`, MSRV policy.
- Done when: `cargo doc --no-deps` has no warnings; examples compile in CI.

**X3 — Release readiness** ⛔
- Depends: all previous tasks.
- Produce: release checklist in `docs/RELEASE.md`; coverage ≥ 85 % codec and ≥ 75 % link/server
  (`cargo llvm-cov`); `cargo deny` clean; `cargo publish --dry-run` for the three crates.
- Gate (human only): similarity check against lib60870 by a third party, legal review,
  crates.io name reservation and publication, git push and tag.
- Done when: maintainer approves and publishes 1.0.0.

---

## 5. Definition of "ready to use" (1.0)

- Client and multi-client server interoperate with the reference peers on 100 % of the bench
  matrix, in both directions, with and without TLS.
- Every in-profile type ID encodes and decodes; profile violations are rejected as the standard
  requires; no panic on any input (fuzzing campaign passed).
- Redundancy switchover without data loss under load.
- Public API documented with examples; binaries `igs104-client` and `igs104-server` usable.
- Every non-trivial behaviour traceable to a clause or an entry of `PROVENANCE.md`.

---

## 6. Progress

| Task | Status |
| --- | --- |
| F1 Workspace skeleton | [x] |
| F2 Continuous integration | [x] |
| F3 Code generation from the profile | [x] |
| F4 Interop bench skeleton ⛔ | [x] |
| F5 AI log and questions | [x] |
| C1 Primitive formats | [x] |
| C2 Information elements | [x] |
| C3 ASDU header | [x] |
| C4 ASDU bodies | [x] |
| C5 APCI framing | [x] |
| C6 Property tests | [x] |
| C7 Fuzzing ⛔ | [ ] |
| L1 Parameters | [x] |
| L2 State machine core | [x] |
| L3 Timers and tests | [x] |
| L4 Interop APCI ⛔ | [x] |
| L5 Fault injection interop | [x] |
| T1 Transport | [x] |
| T2 Client API | [ ] |
| T3 `igs104-client` binary | [ ] |
| T4 Client interop matrix ⛔ | [ ] |
| S1 Process image | [ ] |
| S2 Server | [ ] |
| S3 Redundancy groups | [ ] |
| S4 `igs104-server` binary | [ ] |
| S5 Server interop, load and switchover ⛔ | [ ] |
| X1 TLS | [ ] |
| X2 Documentation and examples | [ ] |
| X3 Release readiness ⛔ | [ ] |

Suggested order: F1, F5, F2, F3, C1–C6, L1–L3, T1, F4, L4, L5, T2, T3, T4, S1–S4, S5, X1, X2, C7, X3.
