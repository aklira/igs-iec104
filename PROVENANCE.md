# Provenance

This file links every non-trivial behaviour of `igs-iec104` to its source: a clause of an IEC
standard, an observation test, or third-party code under a permissive licence. The project is a
clean-room implementation: no protocol behaviour without a source recorded here or in an
`// IEC 60870-5-104 §x.y` comment.

The IEC standards are licensed and are not distributed with this repository. The references below
point to clauses; they do not reproduce the text of the standards.

## Reference standards

| Standard | Edition | Use |
| --- | --- | --- |
| IEC 60870-5-104 | 2006 | TCP/IP transport, APCI, ASDU profile |
| IEC 60870-5-101 | 2003 | ASDU definitions, information elements |
| IEC 62351-3 | 2023 | TLS profile |

## Decisions on ambiguities in the standard

Profile of the causes of transmission (COT) accepted for each ASDU type. Two sources in the 104 can
disagree: the "type identification × cause of transmission" matrix in 104 §9.5, and the list of
causes given in the definition of each ASDU (101 §7.3 or 104 §8). They agree for 49 of the 54 types
in the 104 profile; the 5 discrepancies are resolved below.

Confidence: **high** when the sources within the 104 agree; **medium** when the 104 contradicts
itself, to be confirmed by an observation test against other implementations.

### D-001 — No return information for M_PS_NA_1 (TI 20)

- **Decision**: causes 11 and 12 (return information) rejected for TI 20.
- **Sources**: 104 §9.5, matrix (cells marked "not required"); 104 §7.7, the return-information
  service only covers M_SP, M_DP and M_ST. 101 §7.3.1.20 allows them: restriction specific to the
  104 profile.
- **Confidence**: high.
- **Test**: to be written.

### D-002 — Deactivation of C_BO_NA_1 (TI 51)

- **Decision**: causes 8 and 9 (deactivation and its confirmation) accepted for TI 51, as an
  option; a controlled station may choose not to support them.
- **Sources**: 104 §7.7, the command-interruption service includes C_BO; 101 §7.3.2.7 defines these
  causes. The 104 §9.5 matrix shades these two cells, in a shade that differs from the rest of the
  grid; the explicit text is preferred.
- **Out of scope**: does not apply to C_BO_TA_1 (TI 64), for which 104 §8.7 and the matrix both
  exclude these causes.
- **Confidence**: medium (internal contradiction in the 104).
- **Test**: to be written; to be confirmed by observation.

### D-003 — No deactivation for C_CI_NA_1 (TI 101)

- **Decision**: causes 8 and 9 rejected for TI 101.
- **Sources**: 104 §9.5, matrix (cells marked "not required"); 104 §7.8, transmission of integrated
  totals only provides for activation, confirmation and termination. 101 §7.3.4.2 allows them:
  restriction specific to the 104 profile.
- **Confidence**: high.
- **Test**: to be written.

### D-004 — Spontaneous clock synchronization not permitted (TI 103)

- **Decision**: cause 3 (spontaneous) rejected for C_CS_NA_1; only activation and activation
  confirmation are sent.
- **Sources**: 104 §9.5, matrix (cell marked "not permitted"); 104 §7.6. 101 §7.3.4.4 allows it:
  explicit restriction of the 104.
- **Confidence**: high.
- **Test**: to be written.

### D-005 — Causes for F_SC_NB_1 QueryLog (TI 127)

- **Decision**: sent with the causes of 104 §8.9 only (13, 44 to 47). Cause 5 (request) is never
  sent; whether to tolerate it on reception is still to be decided.
- **Sources**: 104 §8.9 (ASDU definition); the 104 §9.5 matrix leaves cause 5 possible; 104 §7.11
  does not mention QueryLog.
- **Confidence**: medium (internal contradiction in the 104).
- **Test**: to be written; to be confirmed by observation.

### D-006 — Day of week 0 in CP56Time2a

- **Decision**: day of week 0 ("not used") is accepted on encode and decode; 1 to 7 are the days
  from Monday to Sunday; values above 7 are rejected.
- **Sources**: 101 §7.2.6.18: day of week 0 is "not used", 1 to 7 are used. The range given in
  `formats.yaml` (1..7) omits 0 and contradicts the note of the same file, so the 101 text prevails.
- **Confidence**: high.
- **Test**: `formats::times` tests `cp56_min` (day of week 0 accepted) and
  `cp56_rejects_values_outside_each_range` (day of week 8 rejected).

### D-007 — Reserved bits of the time tags

- **Decision**: reserved bits (RES2, RES3, RES4 of CP56Time2a) are written as 0; on decode they are
  ignored and any value is accepted. RES1 is not reserved: it is GEN (see the next paragraph).
- **Sources**: 101 §7.2.6.18 and §7.2.6.19 name the reserved bits without giving a rule for the
  receiver. 60870-5-4 §6.8 gives the layout only.
- **Confidence**: medium (silent standard; the choice keeps the decoder tolerant).
- **Test**: `formats::times` test `cp56_reserved_bits_are_ignored_on_decode`.

RES1 of CP24Time2a and CP56Time2a is GEN in 101 §7.2.6.18: 0 for a genuine time, 1 for a
substituted time. The API names it `substituted`.

### D-008 — All-zero range time in QueryLog (F_SC_NB_1)

- **Decision**: the range times RangeStartTime and RangeStopTime are CP56Time2a, except that seven
  octets set to zero mean "no bound on this side" (`RangeTime::Unbounded`). Such a value is never
  read as a time, since a CP56Time2a with month 0 is invalid.
- **Sources**: 104 §8.9, table of RangeStartTime and RangeStopTime: "0 (all zeros)" in the column
  of the range bound, and the rows for an open start or an open end of range.
- **Confidence**: high.
- **Test**: `elements::tests::range_time_unbounded_and_bounded`.

### D-009 — STATUS of file: range 0 to 32 in five bits

- **Decision**: the STATUS field of SOF is accepted from 0 to 31. Five bits cannot carry the value 32
  that the text lists, so the value is not representable and `Sof::new(32)` fails.
- **Sources**: 101 §7.2.6.38: STATUS UI5[1..5] with range 0..32. The field width gives 0..31.
- **Confidence**: medium (the range in the text does not match the field width; the field width
  wins because it is what a receiver can see).
- **Test**: `elements::tests::file_elements` (`Sof::new(32)` is rejected).

### D-010 — Largest ASDU: 249 octets

- **Decision**: an ASDU is at most 249 octets. Longer ASDUs are rejected on decoding
  (`DecodeError::TooLong`) and on encoding (`EncodeError::TooLong`).
- **Sources**: 104 §5, APCI: the APDU length field counts the four control octets plus the ASDU,
  and its maximum is 253, so the ASDU is at most 249. 104 §9.5 states the 253 maximum for the APDU
  length field, which matches. The generated `MAX_APDU_LENGTH` is 253 (field value).
- **Confidence**: high.
- **Test**: `asdu::tests::decoding_rejects_an_asdu_over_249_octets`.

### D-011 — Framing errors end the stream

- **Decision**: a frame that does not start with 68H, or whose length is outside 4 to 253, is a
  framing error. The frame decoder then stays failed: every later call returns the same error, and
  the connection must be closed. It never tries to resynchronise on a later 68H.
- **Sources**: 104 §5 defines the start octet, the length (4 to 253) and the control field. It does
  not say what a receiver does after a framing error: the text read for this decision is silent.
- **Confidence**: medium (the standard is silent; the choice avoids decoding from a wrong boundary).
- **Test**: `apci::tests::a_bad_start_octet_is_a_sticky_framing_error`,
  `apci::tests::lengths_outside_4_to_253_are_framing_errors`.

### D-012 — Recommended ranges are warnings, not errors

- **Decision**: the ranges that the standard gives as maximum ranges are errors (t0, t1, t2 from 1
  to 255 s, t2 below t1, k and w from 1 to 32767, whole seconds). Two ranges the standard calls
  recommendations are reported, but accepted: t3 above 48 hours, and w above two thirds of k.
  The lower bound of t3 (1 s) is an error: it is the resolution of the parameter, and a zero
  time-out is not a time-out.
- **Sources**: 104 §5.5 (maximum range of k and w; "recommendation: w should not exceed two-thirds
  of k"); 104 §9.6, definition of time-outs (maximum range for t0 to t2; "recommended range for
  time-out t3: 1 s to 48 h, resolution 1 s").
- **Confidence**: high for the split between maximum ranges and recommendations; the wording of the
  standard is explicit in both places.
- **Test**: `config::tests::window_above_two_thirds_of_k_is_a_recommendation_not_an_error`,
  `config::tests::t3_above_48_hours_is_a_recommendation_not_an_error`.

### D-013 — Errors that end the connection

- **Decision**: three errors close the connection rather than being tolerated: an N(R) that
  acknowledges frames never sent, an I frame whose N(S) is not the next number expected, and an
  ASDU or frame that cannot be decoded. The session then accepts no further event.
- **Sources**: 104 §5.1 defines the valid acknowledgement and the sequence numbers, and is silent
  on an invalid one (QUESTIONS.md Q-003, Q-005). 101 §7.2.6 says an ASDU with undefined values is
  discarded, which would keep the connection (Q-006). The prepared text gives no other rule.
- **Confidence**: medium. The standard is silent or differs, and the maintainer chose the
  conservative reaction: a connection in doubt is closed, and the peer reconnects.
- **Test**: `session::tests::an_acknowledgement_of_frames_never_sent_closes_the_connection`,
  `session::tests::an_i_frame_with_an_unexpected_send_number_closes_the_connection`,
  `session::tests::a_frame_that_cannot_be_decoded_closes_the_connection`.

### D-014 — Confirmed means acknowledged by the link

- **Decision**: an I frame, and so its ASDU, is confirmed when the peer's N(R) covers it. When a
  connection ends, the ASDUs that were not confirmed are handed to the application as
  `Delivery::Unacknowledged`, once. The library never sends them again on its own: the redundancy
  group does not resend a command. A duplicate is possible when the peer received a frame whose
  acknowledgement had not been sent yet. Without loss means that every unconfirmed ASDU reaches the
  application, and that the points are read again after each switchover (the station interrogation).
- **Sources**: 104 §5.1 (the acknowledgement of frames, N(R)); §10.5 (the retransmission of commands is
  left to the application; the interrogation that follows a switchover); §10.6 (the retransmission of
  unconfirmed user data). None of these clauses defines "confirmed" (QUESTIONS.md, Q-018).
- **Confidence**: medium. The standard uses "confirmed" without a definition. The link acknowledgement
  is the only confirmation that exists for every type of ASDU: the application confirmations (ACTCON,
  ACTTERM) cover commands and interrogations, not monitoring data.
- **Test**: `tests/transport.rs`, `an_asdu_the_peer_does_not_acknowledge_is_returned_when_the_connection_ends`
  (the hand-back, and no frame sent after it). The group's part, that it does not resend, is in the
  code and not yet tested by a redundancy test (an endpoint that does not acknowledge is needed).

## Third-party code and sources

| Source | Licence | Use | Provenance |
| --- | --- | --- | --- |
| `iec104` crate (Matheus Zaniolo) | MIT, `LICENSE` file checked | Possible inspiration or code | To be confirmed with the author |
