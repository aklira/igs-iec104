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
| Another common address | mirror with 46; the global address 65535 is answered | `respond` `an_unknown_common_address_is_mirrored_with_cause_46`, `a_request_to_the_global_address_is_answered_by_the_station`; `tests/server.rs` `a_request_for_another_station_is_mirrored_with_cause_46` |
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

## Redundancy and TLS

| Function | Tests |
| --- | --- |
| Redundancy group of the station: one started connection, switchover, unacknowledged events sent again, answers of a closed connection not sent again (§10, D-021) | `src/server/tests.rs` (switchover tests, among them `the_answers_of_a_connection_are_not_sent_again_on_the_next_one`); `tests/server.rs` `a_second_started_connection_supersedes_the_first_and_takes_the_events` |
| Redundant controlling station (`RedundantClient`) | `src/redundancy/tests.rs` |
| TLS (IEC 62351-3) with the OpenSSL backend: every mandatory item of the profile | `tests/tls_openssl.rs` `every_mandatory_item_of_the_profile_is_negotiated`; the gaps of the `rustls` backend are in `CHANGELOG.md` |
