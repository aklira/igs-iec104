# Supported actions

This matrix lists what the client (`igs_iec104::client::Client`) and the controlled station
(`igs_iec104::server::Server`) do, by procedure and type identification, and the test that covers
each row. What the library does not do is listed as such. Clauses are those of IEC 60870-5-104
unless another part is named.

The matrix describes the code of the commit it is part of. A change of behaviour updates the
matrix in the same commit.

Test paths are relative to `crates/igs-iec104/` unless they name another crate. `respond` is
`src/server/respond/tests.rs`, `image` is `src/process_image/tests.rs`, and `procedures` is the
test module of `src/procedures.rs`.

## Link layer

| Function | Client | Station | Tests |
| --- | --- | --- | --- |
| STARTDT, STOPDT (§5.3) | sends act, waits for con | answers con | `tests/client.rs` `start_and_stop_follow_the_confirmations`; `tests/transport.rs` `the_transfer_state_is_reported_to_the_application` |
| TESTFR after t3, close after t1 (§5.2, §9.6) | yes | yes | `tests/transport.rs` `an_idle_connection_is_tested_after_t3_and_closed_after_t1`, `a_test_confirmation_keeps_the_connection_open_and_restarts_t3` |
| STARTDT not confirmed within t1 | closes the connection | — | `tests/transport.rs` `a_start_act_without_confirmation_closes_the_connection_after_t1` |
| Reconnection | with a back-off policy | — | `tests/client.rs` `the_client_reconnects_after_the_station_drops_the_connection` |

## Procedures of the controlling station (client)

Each procedure is a method of `Client`, built by `igs_iec104::procedures`. A method returns once the
request is queued; the answer arrives as an event and can be judged with `igs_iec104::answer::judge`.

| Procedure | Type | Cause sent | Method | Tests |
| --- | --- | --- | --- | --- |
| General interrogation (§7.5) | C_IC_NA_1, QOI 20 | 6 | `interrogate(Qoi::STATION)` | `tests/client.rs` `general_interrogation_is_confirmed_and_terminated`; `procedures` `interrogation_is_an_activation_at_the_zero_address` |
| Group interrogation 1 to 16 (§7.5) | C_IC_NA_1, QOI 21 to 36 | 6 | `interrogate(Qoi::group(n))` | `procedures` `a_group_interrogation_carries_the_qualifier_of_its_group` |
| Counter interrogation (§7.8) | C_CI_NA_1, any QCC | 6 | `counter_interrogation` | `tests/client.rs` `counter_interrogation_is_confirmed_and_terminated` |
| Read (§7.2) | C_RD_NA_1 | 5 | `read` | `tests/client.rs` `read_is_answered_with_the_object`; `procedures` `read_is_a_request_of_one_object` |
| Clock synchronization (§7.6) | C_CS_NA_1 | 6 (D-004) | `clock_sync` | `tests/client.rs` `clock_synchronization_is_confirmed_as_an_activation`; `procedures` `clock_sync_is_an_activation_and_never_spontaneous` |
| Test command (§7.10) | C_TS_TA_1 | 6 | `test_command` | `tests/client.rs` `test_command_is_confirmed` |
| Single command (§7.7) | C_SC_NA_1, C_SC_TA_1 | 6 | `single_command` | `tests/client.rs` `single_command_is_selected_then_executed`; `procedures` `a_command_with_a_time_tag_uses_the_time_tagged_type` |
| Double command (§7.7) | C_DC_NA_1, C_DC_TA_1 | 6 | `double_command` | `tests/client.rs` `double_regulating_and_set_point_commands_are_confirmed` |
| Regulating step command (§7.7) | C_RC_NA_1, C_RC_TA_1 | 6 | `regulating_step` | `tests/client.rs` `double_regulating_and_set_point_commands_are_confirmed` |
| Set-point, normalized (§7.7) | C_SE_NA_1, C_SE_TA_1 | 6 | `set_point_normalized` | `tests/client.rs` `double_regulating_and_set_point_commands_are_confirmed` |
| Set-point, scaled (§7.7) | C_SE_NB_1, C_SE_TB_1 | 6 | `set_point_scaled` | `tests/client.rs` `double_regulating_and_set_point_commands_are_confirmed` |
| Set-point, short floating point (§7.7) | C_SE_NC_1, C_SE_TC_1 | 6 | `set_point_float` | `tests/client.rs` `double_regulating_and_set_point_commands_are_confirmed` |
| Any other ASDU of the profile | as built by the caller | as built | `send_asdu` | `procedures` `every_command_is_an_activation` (the builders); the codec test below (the encoding) |

The command methods take the select or execute qualifier from their argument (QOC or QOS) and
send a time-tagged type when a time is given. A command with a select is answered by an activation
confirmation; its execution, sent separately, by a confirmation and a termination.

Not offered as a method (possible with `send_asdu`): bitstring command (C_BO_NA_1, C_BO_TA_1),
reset process (C_RP_NA_1), parameters (P_ME_NA_1, P_ME_NB_1, P_ME_NC_1, P_AC_NA_1), file transfer
(F_ types), and deactivation (cause 8) of a request.

### What the client receives

The client decodes every type identification of the 104 profile and delivers each ASDU as
`Delivery::Asdu`. It keeps no values.

| Function | Tests |
| --- | --- |
| Decoding of every in-profile type | `igs-iec104-codec/src/asdu/tests.rs` `every_in_profile_type_has_a_vector_that_round_trips` |
| Transmission of a request reported (`Delivery::Transmitted`), before its answer; not reported by a controlled session | `tests/transport.rs` `only_a_controlling_session_reports_its_transmissions`, `an_asdu_the_peer_does_not_acknowledge_is_returned_when_the_connection_ends` |
| Requests made without a connection queued (default) or discarded (`OfflineRequests::Discard`) | `tests/client.rs` `a_request_made_without_a_connection_waits_for_the_next_one_by_default`, `a_request_made_without_a_connection_is_discarded_with_the_discard_policy` |
| Every delivery of a connection comes before its `Disconnected` event | `tests/client.rs` `the_deliveries_of_a_connection_come_before_its_disconnection` |
| Judging the answer to a request (confirmation, negative confirmation, causes 44 to 47) | `src/answer.rs` tests, for example `a_negative_confirmation_refuses_with_its_cause`, `a_cause_of_44_to_47_refuses_the_request` |

## Answers of the controlled station (server)

The station answers from its process image, through the `Responder`. Commands go to the
application's `Handler`, which accepts or refuses them.

| Request | Answer | Tests |
| --- | --- | --- |
| End of initialization (101 §7.3.3.1) | M_EI_NA_1, cause 4, when a connection first starts its transfer | `tests/server.rs` `the_station_announces_its_end_of_initialization_when_the_transfer_starts`; `respond` `the_end_of_initialization_reports_its_cause` |
| General interrogation, C_IC_NA_1 QOI 20 (§7.5) | confirmation (7), every point in its plain form with cause 20, termination (10) | `respond` `a_general_interrogation_is_confirmed_answered_and_terminated`; `tests/server.rs` `a_general_interrogation_is_confirmed_answered_and_terminated` |
| Group interrogation, QOI 21 to 36 (§7.5) | the points of the group, with cause 21 to 36 | `respond` `a_group_interrogation_answers_the_points_of_its_group`; `tests/server.rs` `a_group_interrogation_answers_only_the_points_of_its_group` |
| Interrogation with another QOI | negative confirmation | `respond` `an_interrogation_with_an_unknown_qualifier_is_refused` |
| Counter interrogation, C_CI_NA_1, read, general (5) or groups 1 to 4 (§7.8) | confirmation, the counters (M_IT_NA_1) with cause 37 to 41, termination | `respond` `a_counter_interrogation_answers_the_counters_and_refuses_freezing`; `tests/server.rs` `a_counter_interrogation_answers_the_counters` |
| Counter freeze or reset | negative confirmation | `respond` `a_counter_interrogation_answers_the_counters_and_refuses_freezing` |
| Read, C_RD_NA_1 (§7.2) | the point in its plain form with cause 5; mirror with 47 for an unknown address, with 45 when the type does not allow cause 5 | `respond` `a_read_answers_the_point_or_mirrors_an_unknown_address`; `tests/server.rs` `a_read_is_answered_with_the_point_and_an_unknown_address_is_refused` |
| Clock synchronization, C_CS_NA_1 (§7.6) | confirmation carrying the station's time from before the synchronization; negative when the handler refuses | `respond` `a_clock_synchronization_is_confirmed_with_the_time_from_before_it`; `tests/server.rs` `a_clock_synchronization_is_confirmed_with_the_time_of_the_station` |
| Test command, C_TS_TA_1 (§7.10) | confirmation that mirrors the request | `respond` `a_test_command_is_confirmed_with_the_request_it_answers`; `tests/server.rs` `a_test_command_is_confirmed` |
| Select of a command: C_SC, C_DC, C_RC, C_SE_NA, C_SE_NB, C_SE_NC, plain or time-tagged (§7.7) | confirmation; the selection is kept until its time-out | `respond` `every_command_type_is_selected_then_executed`, `a_selection_expires_after_its_time_out` |
| Execution of a selected command | confirmation, then termination | `respond` `every_command_type_is_selected_then_executed`, `an_execution_consumes_its_selection_and_needs_another_one`; `tests/server.rs` `a_single_command_is_selected_then_executed_with_its_time_tag`, `a_set_point_command_is_confirmed_and_terminated` |
| Execution without a kept selection (direct execution) | negative confirmation | `respond` `a_direct_execution_without_a_selection_is_refused`; `tests/server.rs` `a_direct_execution_without_a_selection_is_refused` |
| Command refused by the handler | negative confirmation, or mirror with 47 for an unknown address | `respond` `a_refused_selection_is_a_negative_confirmation`, `a_command_to_an_unknown_address_is_mirrored_with_cause_47`; `tests/server.rs` `a_command_the_application_refuses_is_a_negative_confirmation` |
| More than 16 selections kept on a connection | negative confirmation | `respond` `the_selections_of_a_connection_are_bounded` |
| Another common address | mirror with 46 when the listener serves no station of that address; the global address 65535 is answered for the first station | `respond` `an_unknown_common_address_is_mirrored_with_cause_46`, `a_request_to_the_global_address_is_answered_by_the_station`; `tests/server.rs` `a_request_for_another_station_is_mirrored_with_cause_46` |
| A type, direction, form or cause the profile does not allow | mirror with 44 (type, direction, SQ = 1 form, Q-015) or 45 (cause) | `respond` `a_profile_violation_is_mirrored_with_cause_44_or_45`; `tests/server.rs` `a_message_the_station_cannot_place_is_mirrored_with_cause_44` |
| Deactivation (cause 8) | negative deactivation confirmation (9) | `respond` `a_deactivation_is_refused_with_a_negative_deactivation_confirmation` |

The termination of an execution follows its confirmation at once: the station does not wait for
the application to complete the operation.

### Not handled by the station

These requests are refused with a negative activation confirmation. The test covers the bitstring
command; the other types take the same branch of the responder.

| Request | Tests |
| --- | --- |
| Bitstring command C_BO_NA_1, C_BO_TA_1; reset process C_RP_NA_1; parameters P_ME_NA_1, P_ME_NB_1, P_ME_NC_1, P_AC_NA_1 | `respond` `an_unsupported_command_is_refused_with_a_negative_confirmation` (C_BO_NA_1) |

File transfer (F_ types) is not implemented. A request or a command with more than one object is
refused with a negative confirmation.

### Monitoring types the station publishes

A point of the process image (`PointValue`) is published in its plain type, or in its time-tagged
type (CP56Time2a) when the point is registered as time-tagged. Interrogations and reads always
carry the plain type. A counter is answered only by a counter interrogation.

| Point value | Plain type | Time-tagged type | Tests |
| --- | --- | --- | --- |
| Single point | M_SP_NA_1 | M_SP_TB_1 | `image` `every_point_type_is_published_in_its_plain_and_time_tagged_forms`, `a_time_tagged_point_reports_the_time_of_the_change` |
| Double point | M_DP_NA_1 | M_DP_TB_1 | `image` `every_point_type_is_published_in_its_plain_and_time_tagged_forms` |
| Step position | M_ST_NA_1 | M_ST_TB_1 | same |
| Bitstring of 32 bits | M_BO_NA_1 | M_BO_TB_1 | same |
| Normalized value | M_ME_NA_1 | M_ME_TD_1 | same |
| Scaled value | M_ME_NB_1 | M_ME_TE_1 | same |
| Short floating point value | M_ME_NC_1 | M_ME_TF_1 | same |
| Integrated total | M_IT_NA_1 | M_IT_TB_1 | same |
| Packed single points with status change detection | M_PS_NA_1 | none (refused) | same; `image` `a_packed_point_cannot_be_time_tagged` |
| Normalized value without quality | M_ME_ND_1 | none (refused) | same |

Not published: the protection equipment events (M_EP_TD_1, M_EP_TE_1, M_EP_TF_1) and the file
transfer types.

## Stopping the station

| Function | Tests |
| --- | --- |
| `Server::run_until`: no new connection after the stop; open connections close within the grace period | `tests/server.rs` `a_stop_with_no_connection_returns_at_once`, `a_stop_closes_the_open_connections_within_the_grace_period` |
| Connections still open after the grace period are closed by force, with their transport | `src/server/tests.rs` `the_connections_still_open_after_the_grace_period_are_closed_by_force`, `connections_that_close_in_time_are_not_forced`, `a_transport_is_aborted_with_its_connection` |
| The report counts the events the station still held | `tests/server.rs` `the_events_no_connection_took_are_counted_at_the_stop` |

## Connection events of the station

With `ServerConfig::with_events`, the station reports its connections on a stream taken once with
`Server::events`. A full stream drops the event and counts it; the station never waits.

| Event | Tests |
| --- | --- |
| `Connected` (with the peer address), `Transfer` (STARTDT, STOPDT), `Closed` (with its end) | `tests/server.rs` `the_station_reports_a_connection_its_transfer_and_its_end` |
| `Superseded` by a switchover (§10.7) | `tests/server.rs` `the_station_reports_the_connection_a_switchover_supersedes` |
| `Closed` at a stop of the station; `Forced` for a connection closed by force | `tests/server.rs` `a_stop_reports_the_end_of_each_connection`; `src/server/tests.rs` `a_connection_dropped_without_an_end_is_reported_as_closed_by_force` |
| A full stream drops and counts, and the station still answers | `tests/server.rs` `a_full_event_stream_drops_and_counts_without_slowing_the_station` |

## Redundancy and TLS

| Function | Tests |
| --- | --- |
| Redundancy group of the station: one started connection, switchover, unacknowledged events sent again, answers of a closed connection not sent again (§10, D-021) | `src/server/tests.rs` (switchover tests, among them `the_answers_of_a_connection_are_not_sent_again_on_the_next_one`); `tests/server.rs` `a_second_started_connection_supersedes_the_first_and_takes_the_events` |
| Redundant controlling station (`RedundantClient`) | `src/redundancy/tests.rs` |
| ASDUs of a station kept in order across a full window and a switchover: the ASDUs queued behind one refused for a full window are refused too, in order; the commands queued when a connection ends go to the connection that takes over | `tests/transport.rs` `an_asdu_queued_behind_one_refused_for_a_full_window_is_not_sent_ahead_of_it`; `docs/campaigns/switchover.md` (the cut under load) |
| Several stations on one listener (`ServerConfig::with_station`, `ServerHandle::station`): each answers the requests of its own common address from its own points, with cause 46 for another address; one redundancy group for all of them; each announces its end of initialization | `tests/server.rs` `two_stations_on_one_listener_answer_each_for_its_own_points`, `a_request_for_an_address_of_neither_station_is_mirrored_with_cause_46`, `the_global_address_is_answered_for_the_first_station`, `the_stations_of_one_listener_share_a_redundancy_group_and_each_announces_its_start`, `a_listener_refuses_an_additional_station_that_is_global_or_repeated` |
| TLS (IEC 62351-3) with the OpenSSL backend: every mandatory item of the profile | `tests/tls_openssl.rs` `every_mandatory_item_of_the_profile_is_negotiated`; the gaps of the `rustls` backend are in `CHANGELOG.md` |

## Campaigns (issue #3)

The campaigns run the station and the controlling station of this library against each other. They are in
`bench/campaign/`, and their reports are in `docs/campaigns/`, which says how to run them.

| Campaign | Status | Report | Program and test |
| --- | --- | --- | --- |
| General interrogation of 10 000 points, under spontaneous traffic on the same connection: the percentiles of the interrogation and of the changes | run on loopback, PASS (every interrogation answered every point) | `docs/campaigns/load.md` | `bench/campaign` `load`; `bench/campaign/run.sh` |
| Switchover under load: two stations in one redundancy group, the connection cut during the load; no change lost, in order, at most one resend | run on loopback, PASS | `docs/campaigns/switchover.md` | `bench/campaign` `switchover`; the regression test `tests/transport.rs` `an_asdu_queued_behind_one_refused_for_a_full_window_is_not_sent_ahead_of_it` |
| Interop with two independent peers, controlling and controlled roles | not run: needs Docker, a Java runtime and `tshark` | `docs/campaigns/README.md` | `bench/tests/` (`test_t4_client_matrix.py`, `test_s5_interop_matrix.py`, `test_s5_load.py`, `test_s5_failover.py`) |

## rustls backend: conformance (issue #4)

The rustls backend is not conformant with IEC 62351-3: a profile claim must name the OpenSSL backend (README.md).
The table gives each item of the profile that rustls does not provide, and the test that shows its refusal: a client
that offers only that item is refused by the rustls station. The clients are OpenSSL clients, which offer what rustls
cannot. The control test `the_rustls_station_accepts_the_same_client_with_a_suite_of_the_profile` shows that the same
client is accepted with a suite of the profile. Tests are in `tests/tls.rs`.

| Item of the profile | Table | Status in rustls | Test (refusal) |
| --- | --- | --- | --- |
| TLS_RSA_WITH_AES_128_CBC_SHA256 | 9, m | not offered | `the_rustls_station_refuses_a_client_that_offers_only_the_cbc_suite_of_tls_12` |
| TLS_DHE_RSA_WITH_AES_128_GCM_SHA256 | 9, m | not offered (no finite-field DHE) | `the_rustls_station_refuses_a_client_that_offers_only_the_dhe_suite_of_tls_12` |
| TLS_AES_128_CCM_SHA256 | 13, m | not offered | `the_rustls_station_refuses_a_client_that_offers_only_the_ccm_suite_of_tls_13` |
| ffdhe2048 group | 16, m | not offered | `the_rustls_station_refuses_a_client_that_offers_only_the_ffdhe2048_group` |
| rsa_pss_pss_sha256 scheme | 17, 18, m | not offered (rustls has the rsae variant) | `the_rustls_station_refuses_a_client_that_offers_only_the_pss_pss_scheme` |
| TLS 1.2 renegotiation | 11, m | not implemented | no refusal test: the rustls station never starts one, and it has no renegotiation interval to test |
| TLS 1.3 key update | 20, m | not implemented | no refusal test: the rustls station never sends a KeyUpdate, and it has no key update interval to test |
| OCSP response validation | 8, o | stapled responses are accepted, not validated | none |
| Trusted CA indication | 12, o | not implemented | none |

The two items without a refusal test (renegotiation and key update) are mandatory items: they are documented
exclusions, but no test shows their refusal. They are covered by the OpenSSL backend (`tests/tls_openssl.rs`).

## Security events of Annex A (IEC 62351-3)

Each of the 36 security events of Annex A has a status: raised by the OpenSSL backend, the default, and
tested, or excluded with its reason. The rustls backend raises the events of its own handshake only; the
gaps of that backend are in `CHANGELOG.md`. The identifiers are those of `src/tls/events.rs`, with their
severity and clause.

26 events are raised and tested; 10 are excluded. Each excluded event is listed below with its reason.
`tests/tls_openssl.rs` is the integration test file; `src/...` names are unit tests.

| Event | Severity | Clause | Status | Test or reason |
| --- | --- | --- | --- | --- |
| TLS_HS_SUCCESS | notice | 6.1 | raised | `tests/tls_openssl.rs` `the_station_answers_an_interrogation_over_tls_13_and_raises_the_notice` |
| TLS_DEPRECATED_VERSION | alarm | 6.2 | raised | `tests/tls_openssl.rs` `a_client_that_proposes_only_tls_11_is_refused_with_the_alarm` |
| TLS_WEAK_VERSION | warning | 6.2 | excluded | The station does not enable TLS 1.0 or 1.1 (table 7, conditional), so no weak version is negotiated; a proposal of TLS 1.0 or 1.1 raises TLS_DEPRECATED_VERSION. |
| TLS_DISALLOWED_VERSION | alarm | 6.2 | raised | `tests/tls_openssl.rs` `a_client_that_proposes_ssl_3_is_refused_with_the_alarm` |
| TLS_VERSION_CHANGE | alarm | 6.2 | excluded | A renegotiation that changes the protocol version is refused by OpenSSL before the application sees it, so the station never reaches the event. Not checked by a test (`docs/ai-log/X1.md`). |
| TLS_NO_PEER_CERT | alarm | 6.4.3 | raised | `tests/tls_openssl.rs` `a_client_without_a_certificate_is_refused_and_the_station_raises_no_peer_certificate` |
| TLS_NO_LOCAL_CERT | alarm | 6.4.3 | raised | `src/tls/settings.rs` `an_identity_without_a_key_is_refused_and_raises_the_event` (`Identity::new`, shared by both backends) |
| TLS_SESSION_CLOSED-REV | alarm | 6.4.4.4 | raised | `tests/tls_openssl.rs` `a_revoked_client_certificate_is_refused_with_the_crl_and_both_events_are_raised` |
| TLS_DISALLOWED_CIPHER | warning | 7.3 | raised | `tests/tls_openssl.rs` `a_client_offering_only_a_disallowed_suite_raises_the_warning`; `src/tls/client_hello.rs` `a_hello_offering_only_disallowed_suites_raises_the_event` |
| TLS_SESSIONID_EXPIRED_FULL_HS | warning | 7.4.4 | excluded | A resumption attempt with an expired session ID: the OpenSSL message callback does not see it, so the station cannot raise it from the handshake. Not implemented. |
| TLS_NO_RENEG | alarm | 7.4.5 | raised | `src/tls/openssl_backend/tests.rs` `a_peer_that_refuses_renegotiation_ends_the_session_with_the_alarm`, `a_peer_that_does_not_answer_a_renegotiation_ends_the_session_with_the_alarm` |
| TLS_NO_RENEG_SIG | warning | 7.5.2 | excluded | A peer without secure renegotiation (RFC 5746). An OpenSSL client always sends the extension, and an OpenSSL station refuses the unsafe renegotiation, so no test peer reaches the event (`docs/ai-log/X1.md`). |
| TLS_NO_RENEG_TICKET | alarm | 7.5.2 | excluded | The same peer, in a renegotiated handshake. Same reason as TLS_NO_RENEG_SIG. |
| TLS_NO_TR_CA_MATCH_S | alarm | 7.5.3 | excluded | The trusted CA indication (clause 7.5.3) is not implemented. |
| TLS_NO-SIG_ALGO_EXT | warning | 7.5.4 | raised | `tests/tls_openssl.rs` `a_client_without_signature_algorithms_raises_the_warning` |
| TLS_DEP_SIG_ALGO | warning | 7.5.4 | raised | `tests/tls_openssl.rs` `a_client_that_signals_sha_1_with_rsa_raises_the_warning_and_is_accepted` |
| TLS_NO_EPSK_MODE | alarm | 7.5.7 | raised | `tests/tls_openssl.rs` `a_cbc_suite_without_encrypt_then_mac_ends_the_session_with_the_alarm` (mnemonic as printed, Q-020) |
| TLS_NO_ENCRYPT-THEN-MAC | warning | 8.3.2 | raised | `tests/tls_openssl.rs` `a_client_that_offers_only_psk_ke_raises_the_alarm_and_is_not_established` (mnemonic as printed, Q-020) |
| TLS_NO_SK_UPDATE | alarm | 8.4 | raised | `src/tls/openssl_backend/tests.rs` `a_peer_that_does_not_answer_a_key_update_ends_the_session_with_the_alarm` |
| TLS_NO_TR_CA_MATCH_SC | alarm | 8.8.8 | excluded | The trusted CA indication (clause 8.8.8) is not implemented. |
| TLS_EARLY-DATA | warning | 8.8.11 | raised | `tests/tls_openssl.rs` `early_data_in_the_handshake_raises_the_warning` |
| TLS_CERT_SIZE_MISMATCH | alarm | 6.4.2 | raised | `tests/tls_openssl.rs` `a_server_certificate_above_the_size_limit_is_refused_by_the_client` |
| TLS_NO_CA_MATCH | alarm | 6.4.4.2 | raised | `tests/tls_openssl.rs` `a_client_certificate_from_an_unknown_ca_is_refused_and_the_station_raises_no_ca_match` |
| TLS_NO_TRUSTED_CERT_MATCH | alarm | 6.4.4.3 | excluded | The lists of individual trusted certificates (clause 6.4.4.3, "should") are not implemented. |
| TLS_CERT_REVOKED | alarm | 6.4.4.4 | raised | `tests/tls_openssl.rs` `a_revoked_client_certificate_is_refused_with_the_crl_and_both_events_are_raised` |
| TLS_NO_CRL | warning | 6.4.4.4.2 | raised | `tests/tls_openssl.rs` `the_lack_of_a_crl_of_the_issuer_is_a_warning_and_the_client_is_accepted` |
| TLS_CRL_EXP | warning | 6.4.4.4.2 | raised | `tests/tls_openssl.rs` `an_expired_crl_is_a_warning_and_the_client_is_accepted` |
| TLS_OCSP_RES_EXP | warning | 6.4.4.4.3 | excluded | OCSP is optional (clause 6.4.4.4.3) and not implemented: no OCSP response is checked. |
| TLS_OCSP_RESP_UNAVAIL | warning | 6.4.4.4.3 | excluded | OCSP is optional (clause 6.4.4.4.3) and not implemented: no responder is queried. |
| TLS_CERT_EXP | alarm | 6.4.4.5 | raised | `tests/tls_openssl.rs` `an_expired_client_certificate_is_refused_and_the_station_raises_the_expiry` |
| TLS_SIG_ALG_MISMATCH | alarm | 6.4.4.6 | raised | `tests/tls_openssl.rs` `a_peer_with_no_signature_algorithm_in_common_is_refused_with_the_alarm` |
| TLS_SIG_V_FAILED | alarm | 6.4.4.6 | raised | `tests/tls_openssl.rs` `a_certificate_whose_signature_does_not_verify_is_refused_with_the_alarm` |
| TLS_SHORT_RSA_KEY | alarm | 7.4.3 | raised | `tests/tls_openssl.rs` `an_rsa_key_of_1024_bits_is_refused_unless_the_policy_allows_it` |
| TLS_MIN_KEY | warning | 7.4.3 | raised | `tests/tls_openssl.rs` `an_rsa_key_of_1024_bits_is_refused_unless_the_policy_allows_it` |
| TLS_SHORT_KEY | alarm | 7.4.3 | raised | `tests/tls_openssl.rs` `an_rsa_key_below_1024_bits_is_refused_even_when_the_policy_allows_1024` |
| TLS_DEP_HASH | warning | 7.4.3 | raised | `tests/tls_openssl.rs` `a_certificate_signed_with_sha_1_is_refused_with_the_warning` |
