# S5 — Two known gaps of the interop matrix: reset and the test command without time tag

## Prompt (summarised)

The S5 matrix recorded two known gaps: the station refuses the reset process command (C_RP_NA_1), and it closes the
connection on a test command without time tag (C_TS_NA_1). The maintainer chose the recommended option for both:
an application hook for the reset, and a cause 44 answer that keeps the connection open for the out-of-profile type.

## Summary

- **Reset** (`server/respond.rs`, `Handler::reset_process`): the station calls the handler with the qualifier. The
  default refuses, so the demonstration station is unchanged: a negative confirmation of cause 7. An accepting handler
  gets a positive confirmation, with no termination (the profile allows no cause 10 for C_RP_NA_1).
- **Out-of-profile type** (`igs-iec104-codec`, `Body::Unsupported`): `Asdu::decode` keeps an ASDU whose type is in the
  catalogue but outside the 104 profile: its VSQ and octets are kept, and it is written back unchanged. The station's
  profile check mirrors it with cause 44, the same body, and the connection stays open. Before, the decoder rejected it
  and the connection closed (D-013). A malformed ASDU of an in-profile type still closes the connection.
- **Matrix** (`bench/tests/test_s5_interop_matrix.py`): the row of the test command without time tag now expects a
  refusal with cause 44; the reset row still expects the refusal of cause 7, which the demonstration station gives.

## Tests

- Codec: `asdu::tests::type_outside_the_profile_is_kept_with_its_octets_and_written_back_as_received`,
  `apci::tests::an_unsupported_asdu_is_kept_and_the_stream_goes_on`.
- Station: `server::respond::tests::a_test_command_outside_the_profile_is_mirrored_with_cause_44_and_nothing_else`,
  `server::respond::tests::a_reset_process_command_is_confirmed_once_and_refused_when_the_handler_refuses_it`.
- Bench: `test_s5_interop_matrix.py`, 13 passed (the j60870 peer against the static station).
- Workspace: fmt, clippy with `-D warnings`, all tests, `cargo deny`: passing.

## Decisions

See `PROVENANCE.md`, D-031. Q-006 is updated: out-of-profile types are answered; malformed in-profile ASDUs still close.

## Points for the reviewer

- `Body` gains a variant: an exhaustive match on `Body` outside the crates of this repository must add an arm.
- The station answers the out-of-profile test command with cause 44 on the wire, which the j60870 peer reports as a
  refusal. A station that wants to implement the reset process command overrides `Handler::reset_process`.
