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

### D-015 — The time that a peer has to answer a renegotiation

- **Decision**: a renegotiation that this side asked for must be answered within 30 seconds, the
  default t0. The renegotiation is answered when its handshake has finished. A peer that does not
  answer in time, or refuses with a no_renegotiation alert, ends the TLS 1.2 session, and the alarm
  TLS_NO_RENEG is raised.
- **Sources**: 62351-3 clause 7.4.5 (the called entity terminates the connection when it receives no
  ClientHello in answer to a HelloRequest; the termination should raise "alarm: session renegotiation
  interval expired"). The clause gives no time. 104 §9.6 gives the time-out of connection
  establishment, t0, with the default 30 s.
- **Confidence**: medium. The standard leaves the time open. A renegotiation is a handshake, so it is
  bounded as a connection is.
- **Test**: `openssl_backend::tests::a_peer_that_refuses_renegotiation_ends_the_session_with_the_alarm`,
  `openssl_backend::tests::a_peer_that_does_not_answer_a_renegotiation_ends_the_session_with_the_alarm`,
  `openssl_backend::tests::an_idle_tls_12_session_is_renegotiated_by_its_timer_alone`.

### D-016 — The time that a peer has to answer a KeyUpdate

- **Decision**: a KeyUpdate that this side writes with `update_requested` must be answered within 30
  seconds, counted from the write of the message (the same time as D-015). The answer is a KeyUpdate
  with `update_not_requested` from the peer. A peer that does not answer in time ends the TLS 1.3
  session, and the alarm TLS_NO_SK_UPDATE is raised. The next update is due one interval after the
  answer.
- **Sources**: 62351-3 clause 8.4 (the receiver of a KeyUpdate with `update_requested` answers with
  `update_not_requested`; a side that does not receive the answer terminates the connection and raises
  "alarm: session key update interval expired"). The clause gives no time. The fields of the message are
  in RFC 8446, section 4.6.3. 104 §9.6 gives t0, with the default 30 s.
- **Confidence**: medium. The standard leaves the time open, as for D-015. The default interval of 6
  hours is the suggestion of clause 8.4, and it is data.
- **Test**: `openssl_backend::tests::a_tls_13_session_is_updated_and_the_peer_answers`,
  `openssl_backend::tests::a_peer_that_does_not_answer_a_key_update_ends_the_session_with_the_alarm`,
  `settings::tests::the_key_update_interval_defaults_to_six_hours_and_must_not_be_zero`.

### D-017 — A missing or expired CRL is a warning, not a refusal

- **Decision**: when CRLs are configured and none of them is the CRL of the certificate's issuer, the
  warning NoCrl is raised, and the certificate is accepted without a revocation check. When the CRL of
  the issuer has expired (its `nextUpdate` has passed), the warning CrlExpired is raised, and the
  revocation check goes on with that CRL. A revoked certificate is refused as before. With no CRL
  configured at all, no revocation check is made and no warning is raised.
- **Sources**: 62351-3 clause 6.4.4.4.2 (the unavailability of a local CRL raises a warning; neither that
  nor the expiry of a CRL shall terminate the session); clause 6.4.4.4.1 (a revoked certificate is refused).
- **Confidence**: medium. The clause says "unavailable" and does not say what a station with no CRL at
  all should report. A warning for every connection of a station that does not check revocation would
  not say anything useful, so none is raised then.
- **Test**: `tls_openssl::the_lack_of_a_crl_of_the_issuer_is_a_warning_and_the_client_is_accepted`,
  `tls_openssl::an_expired_crl_is_a_warning_and_the_client_is_accepted`. Resolves Q-026.

### D-018 — A certificate signed with SHA-1 is refused

- **Decision**: a certificate whose signature uses SHA-1 (sha1WithRSA, or ecdsa-with-SHA1) raises the
  warning DeprecatedHash, and is refused. This applies to each certificate of the chain.
- **Sources**: 62351-3 clause 7.4.3 (the detection of SHA-1 raises the warning "deprecated hash algorithm
  used"; the support of SHA-1 is deprecated and limited to backward compatibility); clause 7.5.4 (the
  security policies shall be able to disallow optional hashing algorithms).
- **Confidence**: medium. The clause requires the warning only. Refusing is the policy of the station,
  which the standard leaves to the operator.
- **Test**: `tls_openssl::a_certificate_signed_with_sha_1_is_refused_with_the_warning`.

### D-019 — RSA key lengths: the events of clause 7.4.3 read literally

- **Decision**: an RSA key below 2 048 bits is refused, and raises ShortRsaKey (alarm), unless the policy
  allows 1 024-bit keys and the key has at least 1 024 bits. In that case the warning MinimumKeyLength is
  raised and the key is accepted. A key below 1 024 bits is refused in any case, and raises ShortKey
  (alarm) as well as ShortRsaKey. The policy allows 1 024-bit keys only when the settings say so (off by
  default). Only RSA keys are checked: the standard defines no event for an ECDSA key below 256 bits.
- **Sources**: 62351-3 clause 7.4.3 (1 024-bit RSA disabled by default; the alarm "insufficient RSA key
  length (<2 048 bit)" with termination when it is disabled; the warning "minimum key length" when it is
  enabled; the alarm "insufficient key length" with termination for a key below 1 024 bits).
- **Confidence**: medium. The first alarm covers every key below 2 048 bits, so a key below 1 024 bits
  raises it too; the literal reading gives both alarms.
- **Test**: `tls_openssl::an_rsa_key_of_1024_bits_is_refused_unless_the_policy_allows_it`,
  `tls_openssl::an_rsa_key_below_1024_bits_is_refused_even_when_the_policy_allows_1024`.

### D-020 — The events of a ClientHello are raised as it arrives

- **Decision**: a station raises the events of a ClientHello from OpenSSL's message callback, as the
  message arrives, and before the handshake is complete. These are the versions (DisallowedVersion below
  TLS 1.0, DeprecatedVersion for TLS 1.0 and 1.1), the missing signature_algorithm extension, a SHA-1 with
  RSA signature, early data, and psk_ke as the only mode. The two sessions that the standard terminates
  are terminated after the handshake, because the callback cannot end a handshake: a CBC suite without
  encrypt-then-MAC (clause 7.5.7) and psk_ke only (clause 8.3). A peer that never completes its handshake
  is closed by the time-out of the handshake, t0.
- **Sources**: 62351-3 clauses 6.2, 7.5.4, 7.5.7, 8.3 and 8.8.11; RFC 5246 (the signature_algorithm
  extension, section 7.4.1.4.1, and the legacy version); RFC 8446 (supported_versions, section 4.2.1;
  psk_key_exchange_modes, section 4.2.9; early_data, section 4.2.10).
- **Confidence**: medium. The clauses say the session "shall be terminated"; they do not say when.
- **Test**: `client_hello` unit tests, and `tls_openssl::a_client_that_proposes_only_tls_11_is_refused_with_the_alarm`,
  `tls_openssl::a_client_that_proposes_ssl_3_is_refused_with_the_alarm`,
  `tls_openssl::a_client_without_signature_algorithms_raises_the_warning`,
  `tls_openssl::a_client_that_signals_sha_1_with_rsa_raises_the_warning_and_is_accepted`,
  `tls_openssl::early_data_in_the_handshake_raises_the_warning`,
  `tls_openssl::a_client_that_offers_only_psk_ke_raises_the_alarm_and_is_not_established`,
  `tls_openssl::a_cbc_suite_without_encrypt_then_mac_ends_the_session_with_the_alarm`.

### D-021 — The answers of a closed connection are not sent again

- **Decision**: when a connection ends, the station keeps for the connection that takes over only the
  events that it had not acknowledged or sent, which are the spontaneous ASDUs (cause 3) of the image. The
  answers to the requests of the closed connection (confirmations, terminations, the answers to reads) and
  its end of initialization (cause 4) are dropped.
- **Why**: a controlling station takes an answer it did not ask for on its new connection as the answer to
  its new request. A refused execution was followed on the next connection by its stale negative
  confirmation, which refused a select sent there (reproduced with the client binary against the demo
  station). The end of initialization is sent again on each started connection, so a copy from an earlier
  connection would be a duplicate.
- **Sources**: 104 §10.5 and §10.6, applied to the events as the carry of S3 does (docs/ai-log/S3.md). The
  clauses were not read again for the answers.
- **Confidence**: medium. This is the maintainer's choice for the answers; the clauses leave the answers to
  the application for commands (§10.5, as S3 notes).

### D-022 — The values of the DHE GCM suites are the registry values of their names

- **Decision**: TLS_DHE_RSA_WITH_AES_128_GCM_SHA256 is 0x009E, and TLS_DHE_RSA_WITH_AES_256_GCM_SHA384 is
  0x009F. The profile uses these values. The values printed in table 1 of clause 7.2 (0xC0,0x9E and 0x00,0xA1)
  are not used.
- **Sources**: 62351-3 clause 7.2, table 1 (the names and the printed values, as printed); the IANA TLS
  cipher suite registry (the values of the names: 0x009E and 0x009F for the GCM suites; 0xC09E names
  TLS_DHE_RSA_WITH_AES_128_CCM and 0x00A1 names TLS_DH_RSA_WITH_AES_256_GCM_SHA384).
- **Confidence**: high for the values of the names, which the registry gives. The standard's printed values
  conflict with the names: the decision follows the names. The maintainer approved this correction (2026-10-12).
- **Test**: `tls::profile::tests::the_dhe_gcm_suites_have_the_values_of_their_names`.

### D-023 — The mnemonics of clauses 7.5.7 and 8.3.2 are kept as printed

- **Decision**: the events TLS_NO_EPSK_MODE (clause 7.5.7, the encrypt-then-MAC event) and TLS_NO_ENCRYPT-THEN-MAC
  (clause 8.3.2, the PSK event) keep the mnemonics printed in Annex A, table A.1, although the table pairs each
  with the other clause. The meaning of each event is the description of its clause.
- **Sources**: 62351-3 Annex A table A.1; clauses 7.5.7 and 8.3.2.
- **Confidence**: high for the reading of the clauses; the choice of the printed mnemonic is a presentation choice.
- **Test**: `tls::events::tests::the_mnemonics_are_as_printed` (the mnemonics), with the mnemonic of each event
  in its source.

### D-024 — The duplicate identifier 2.8 of Annex A is kept as printed

- **Decision**: the events OCSP responder unavailable (clause 6.4.4.4.3) and certificate expired (clause 6.4.4.5)
  both carry the identifier IEC 62351-3:2.8, as printed. The identifiers 2.9 to 2.14 follow in order.
- **Sources**: 62351-3 Annex A table A.2.
- **Confidence**: medium: the standard prints the duplicate and gives no other number.
- **Test**: `tls::events::tests::identifiers_are_unique_except_the_printed_duplicate_of_2_8`.

### D-025 — The group 62391 of the short-key event is a typo for 62351

- **Decision**: the identifier of the short-key event (clause 7.4.3) is IEC 62351-3:2.13. The printed group
  62391 is read as a typo, as the other rows of the table give 62351-3.
- **Sources**: 62351-3 Annex A table A.2 (the other rows of the table).
- **Confidence**: high.
- **Test**: `tls::events::tests::the_mnemonics_are_as_printed` (the identifier 62351-3:2.13).

### D-026 — 3DES is disallowed, as DES for encryption

- **Decision**: clause 7.3 disallows the suites with DES for encryption. "TLS_*_DES_*" is not limited to the
  names it lists, and 3DES encrypts with DES. The predicate refuses 3DES suites (TLS_RSA_WITH_3DES_EDE_CBC_SHA
  and the like). The station's ciphers do not offer 3DES, so no session negotiates it.
- **Sources**: 62351-3 clause 7.3 (its list is "not limited to" the names given).
- **Confidence**: medium: the literal pattern does not match 3DES; the clause's purpose does. Maintainer decision.
- **Test**: `tls::profile::tests::the_disallowed_names_of_clause_7_3_are_recognised`.
- **Gap, not yet implemented**: clause 7.3 also asks for an alarm when only disallowed suites are proposed in a
  ClientHello, and for the server to ignore a disallowed proposal. The predicate is not applied at the handshake.

### D-027 — A refused handshake on a revoked certificate raises both events

- **Decision**: a revoked certificate that refuses a session establishment raises "certificate validation: revoked
  certificate" on detection, and "session terminated due to revoked certificate" when the refusal ends the
  connection attempt.
- **Sources**: 62351-3 clause 6.4.4.4.1 (an entity receiving a revoked certificate during establishment refuses
  the connection; the detection raises one event, the termination raises the other).
- **Confidence**: medium to high.
- **Test**: `tests/tls_openssl.rs::a_revoked_client_certificate_is_refused_with_the_crl_and_both_events_are_raised`.

### D-028 — The handshake notice is raised on each side

- **Decision**: "handshake successfully performed" is raised by each side when its own handshake completes. In TLS
  1.3 the client may complete its side before the station checks the client certificate, so the client raises the
  notice and the station then refuses the certificate.
- **Sources**: 62351-3 clause 6.1 (the notice on a successful handshake, with no statement per side); clauses
  6.4.3 (certificate checks).
- **Confidence**: medium: the clause does not say whether the notice means both sides.
- **Test**: `tests/tls_openssl.rs` (the TLS 1.2 and TLS 1.3 client certificate tests).

### D-029 — A client without a finite-field group gets the station's Diffie-Hellman parameters

- **Decision**: a TLS 1.2 client that offers the DHE suite and no finite-field group is negotiated the DHE suite.
  The station supplies its own parameters (the RFC 5114 groups offered by the OpenSSL backend). A client that
  offers ffdhe2048 gets the RFC 7919 group, as tested by the mandatory group of TLS 1.3.
- **Sources**: 62351-3 clauses 7.2 (table 1), 7.4.2, 8.3.3 (table 4). The standard does not say which parameters
  the TLS 1.2 DHE suite uses.
- **Confidence**: medium. The test checks the negotiation; it does not inspect the parameters.
- **Test**: `tests/tls_openssl.rs::a_dhe_suite_offered_without_a_finite_field_group_is_negotiated`.

### D-030 — The suites disallowed by clause 7.3 are read from the IANA registry

- **Decision**: the suites disallowed by clause 7.3 are the NULL suite, the suites with MD5 as a component, and
  the suites with DES (DES40 and 3DES included) for encryption. Their IANA values come from the TLS cipher suite
  registry (47 suites, `DISALLOWED_SUITES` in `tls/profile.rs`). A ClientHello that offers only disallowed suites
  raises TLS_DISALLOWED_CIPHER; a ClientHello that also offers an allowed suite does not. The station's own
  cipher list does not contain any of them, so the handshake fails when no allowed suite is offered, and the
  station ignores the disallowed proposals.
- **Sources**: 62351-3 clause 7.3 (the list of disallowed suites, "not limited to" its names; the alarm for a
  ClientHello that proposes only disallowed suites; the server ignores a disallowed proposal); the IANA TLS cipher
  suite registry (values of the names).
- **Confidence**: high for the names and the values; medium for the scope of "not limited to". NULL encryption
  without MD5 or DES is conditional (table 1), so it is not disallowed here.
- **Test**: `tls::client_hello::tests::a_hello_offering_only_disallowed_suites_raises_the_event`,
  `tls::profile::tests::the_disallowed_table_follows_the_names`.
- **Open**: the severity of TLS_DISALLOWED_CIPHER. Annex A prints it as a warning; clause 7.3 calls the event an
  alarm. The code keeps the warning as printed (QUESTIONS.md, Q-030).

## Third-party code and sources

| Source | Licence | Use | Provenance |
| --- | --- | --- | --- |
| `iec104` crate (Matheus Zaniolo) | MIT, `LICENSE` file checked | Possible inspiration or code | To be confirmed with the author |
| OpenMUC j60870 1.8.0 (library, javadoc read from Maven Central) | GNU GPL v3 | Second reference peer of the S5 interop matrix, in a local image run as a separate process; not linked, not distributed | Approved by the maintainer on 2026-10-12 (docs/ai-log/S5.md); no source or class file read |
