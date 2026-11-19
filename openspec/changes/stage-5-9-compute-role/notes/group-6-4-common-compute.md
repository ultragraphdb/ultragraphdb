# Task 6.4: `common_compute`'s reads moved onto `ReadExecutor` (2026-11-19, base `2127e67b`)

`ultragraphdb_common_compute`'s own suites drive every read through `ReadExecutor` with a client's Read, as
`features/quad-reads/executor.feature`'s steps drive it. No test of the crate reaches `ComputeSession` or sends a
ComputeRead. `src/types/compute_session.rs` and its export stay until `ultragraphdb_compute_tests` no longer names it; the
library's non-test code is unchanged.

## The path a read takes

`tests/apart/apart_session.rs`, new, included by the unit, property and BDD targets with the rest of `tests/apart/`:

- `SessionRead`: a client's Read by its pattern's terms, limit and resume key, with what the session takes of the read's
  transaction as it starts the read (tenant, level, partition, identity, version or named version, epoch, principal,
  deadline), the client's `max_frame` and the credit the read's stream starts with. It keeps `ComputeRead`'s builder
  names, so each test states the same inputs. At its start the Read is built by the wire's `Read` builder, read back by
  `ReadView`, taken by `ReadPattern::of_read` and handed over as `ReadRequest::new` and its setters. A pattern
  `ReadPattern::of_read` refuses is answered by that refusal's Error frame on the read's stream, as the session answers
  it. Its `pattern_hash` is `ReadView::pattern_hash` of its Read, the bytes `ComputeReadView::pattern_hash` read
  (`notes/group-5-design.md`, section 3).
- `ReadSession`: one client connection's reads. A task of the test's runtime owns the executor and takes its frames as it
  hands them out, as a connection's serve loop does, so a read's end is taken as it comes. Each read runs on a stream of
  its own. A Result frame not its stream's last spends a frame of credit as the client takes it, and the read is given
  room (`ReadExecutor::ready`) while credit remains; a Credit gives room to a read that waited for it, as `Outbound`
  reports a stream ready; a Cancel is `ReadExecutor::cancel`. Dropped, it drops the executor and its reads, as a
  connection's end does.

The harnesses keep their call shapes over it:

- `ApartCluster::compute`, which served a `ComputeSession` on a runtime of its own thread, is `ApartCluster::session`: a
  `ReadSession` over `ReadExecutor::new(core).with_open(open)` on the test's runtime, where `read_executor_tests` already
  ran its executors.
- `utils::played_cluster::Played`, `played::played_cluster::PlayedCluster` and `played_reads::played_cluster::PlayedCluster`
  hold their `ComputeCore` and no compute listener; `client()` is a `ReadSession` over it, `Played`'s counting its reads
  among the cluster's open requests as its sessions did.
- `apart_reads`, `utils::played_answers`, `played::played_read` and `played_reads::read_answer` build `SessionRead` where
  they built `ComputeRead`, take `ReadSession` where they took `RoleClient`, and grant Credit and Cancel through it.

## Counts

Each target's counts from its log at the base and after:

| Target | Before | After | Difference |
|---|---|---|---|
| `ultragraphdb_common_compute_tests` (unit) | 302 tests | 300 tests | 2 removed: `compute_session_tests.rs` |
| `property` | 18 tests | 17 tests | 1 removed: `test_both_callers_get_one_answer` |
| `bdd` | 38 scenarios, 76 steps | 38, 76 | none |
| `bdd_catalog` | 11 scenarios, 22 steps | 11, 22 | none |
| `bdd_catalog_cap` | 1 scenario, 2 steps | 1, 2 | none |
| `bdd_endpoint` | 22 scenarios, 44 steps | 22, 44 | none |
| `bdd_executor` | 11 scenarios, 22 steps | 11, 22 | none |
| `bdd_held` | 7 scenarios, 14 steps | 7, 14 | none |
| `bdd_quad_reads` | 14 scenarios, 28 steps | 14, 28 | none |
| `bdd_routing` | 17 scenarios, 34 steps | 17, 34 | none |
| `bdd_system_link` | 15 scenarios, 30 steps | 15, 30 | none |
| `bdd_tightening` | 7 scenarios, 14 steps | 7, 14 | none |
| `bdd_timer` | 4 scenarios, 8 steps | 4, 8 | none |

The unit and property targets' test names before and after differ by the three removals below and nothing else.

## Moved

137 unit tests, each with its assertions, inputs and expected values unchanged; only the path its read takes changed. A
test counts as moved when it, or a helper of its file, takes a client's reads (`client()`, `session(`, `holding(`):

- `types/access_tests.rs`: `test_a_bound_subject_alone_reads_each_list_its_predicates_list_names`, `test_a_bound_object_alone_reads_each_reverse_list_its_predicates_list_names`, `test_an_unbound_graph_is_walked_in_the_accesss_keyspace`
- `types/compute_core/compute_core_tests.rs`: `test_a_placement_other_than_the_even_map_refuses_the_read`, `test_every_batch_names_the_cluster`, `test_the_reads_of_a_core_share_its_connection_to_a_partition`, `test_a_frontend_silent_past_the_bound_refuses_the_read`, `test_a_partition_not_reached_refuses_the_read`, `test_reads_arriving_during_a_dial_share_it`, `test_members_listing_compute_processes_reach_the_partition`, `test_the_placement_and_the_members_are_read_at_the_group`
- `types/compute_core/dial_under_way_tests.rs`: `test_a_read_cancelled_while_its_core_dials_leaves_the_next_to_dial`, `test_a_read_waiting_on_a_dial_given_up_dials_for_itself`
- `types/link_tests.rs`: `test_a_frame_out_of_order_closes_the_connection_and_the_next_read_dials`, `test_a_partition_silent_under_a_read_given_up_is_hung`, `test_a_partition_answering_after_a_read_given_up_keeps_its_connection`
- `types/names_tests.rs`: `test_a_read_names_each_object_by_its_dictionary_text`, `test_a_read_names_each_predicate_by_its_dictionary_text`, `test_a_long_literal_is_named_by_its_hashed_text`, `test_a_read_lets_its_texts_go_past_their_count_bound`, `test_a_read_lets_its_texts_go_past_their_byte_bound`
- `types/page_tests.rs`: `test_a_page_names_no_row_past_the_limit`
- `types/partition_rows/partition_rows_fill_tests.rs`: `test_a_read_returns_every_row_in_key_order_frame_after_frame`, `test_a_limit_meeting_the_last_row_hands_no_resume_key`, `test_a_limit_inside_a_page_hands_a_resume_key_the_read_goes_on_from`, `test_a_scan_stopped_at_its_limit_leaves_its_range_to_resume`, `test_a_scan_still_open_at_the_limit_hands_a_resume_key`, `test_a_subject_read_reads_its_predicates_list_then_each_list`, `test_a_subject_read_resumed_goes_on_at_the_next_predicate`, `test_a_read_of_every_graph_walks_each_graph_in_turn`, `test_a_scan_stalled_behind_the_merge_goes_on_where_it_stopped`, `test_a_stall_past_a_one_key_range_goes_on_at_the_next_range`, `test_a_stall_again_where_the_read_went_on_from_refuses_the_read`, `test_a_scan_stalled_before_any_key_refuses_the_read`, `test_a_partition_refusing_a_list_scan_refuses_the_read`, `test_a_scan_stopped_by_its_limit_short_of_a_row_goes_on`
- `types/partition_rows/partition_rows_frames_tests.rs`: `test_a_bound_object_takes_its_record_alone`, `test_an_object_held_under_statements_is_a_row_per_statement`, `test_a_bound_reifier_reads_its_statements_properties`, `test_a_bound_reifier_takes_the_properties_matching_the_pattern`
- `types/partition_rows/partition_rows_plan_tests.rs`: `test_a_bound_iri_object_reads_its_one_record`, `test_a_bound_literal_object_alone_reads_its_reverse_list`, `test_a_bound_vector_object_reads_the_forward_lists`, `test_a_reverse_read_of_no_referrer_reads_no_list_at_the_home`, `test_a_read_resumed_in_a_list_goes_on_after_its_last_row`, `test_a_read_resumed_in_a_predicates_list_asks_from_where_it_stopped`, `test_a_bound_term_the_partition_does_not_name_answers_no_row`, `test_a_read_in_a_named_graph_resumes_in_that_graph`
- `types/partition_rows/partition_rows_terms_tests.rs`: `test_every_unbound_position_is_named_by_the_dictionary`, `test_one_dictionary_scan_names_a_pages_rows`, `test_a_page_whose_names_pass_a_frame_is_named_in_parts`, `test_a_row_that_does_not_read_refuses_the_read_after_the_rows_before_it`, `test_a_limit_ends_with_the_resume_key_of_the_last_row`
- `types/partition_rows/partition_rows_tests.rs`: `test_a_list_gives_each_object_once_in_its_order`, `test_a_limit_takes_the_first_objects_of_the_list`, `test_a_read_reaching_one_partition_names_it`, `test_a_monotonic_fast_read_names_the_epoch_its_partition_chose`, `test_only_a_scan_waiting_for_an_epoch_carries_a_deadline`, `test_every_scan_after_the_first_reads_the_version_it_chose`, `test_a_read_only_read_of_a_version_chosen_before_it_holds_it`, `test_a_read_write_read_holds_no_version`, `test_a_reverse_read_follows_the_referrer_set_from_the_home`, `test_what_a_partition_holds_that_does_not_read_refuses_the_read`, `test_a_referrer_set_answered_over_frames_is_read_to_its_last`
- `types/range_spec_tests.rs`: `test_a_whole_range_is_scanned_alone_at_the_limit`, `test_one_key_ranges_are_scanned_together_at_no_limit`, `test_one_row_wanted_asks_one_one_key_range`
- `types/read_at_tests.rs`: `test_a_snapshot_read_scans_at_its_version_past_its_epoch`, `test_a_named_read_scans_at_its_version_waiting_for_no_epoch`
- `types/read_pins_tests.rs`: `test_a_snapshot_read_holds_its_version_from_its_first_frame`, `test_a_short_latest_read_holds_no_version`, `test_each_partition_holds_its_version`, `test_a_read_going_on_holds_its_version_past_one_scans_grants`, `test_a_stopped_read_opens_no_further_scan`, `test_a_partition_asked_twice_at_one_version_is_held_once`, `test_a_pin_refused_holds_nothing_and_the_read_goes_on`
- `types/read_request_tests.rs`: `test_a_reads_columns_are_its_unbound_slots_and_the_reifier`, `test_a_bound_object_is_read_by_its_referrers`, `test_only_a_read_only_read_with_no_term_waits_on_a_dictionary_scan`, `test_a_read_write_read_names_its_iris_and_holds_no_version`, `test_a_resume_key_is_bound_to_its_transaction`, `test_a_scan_waits_for_its_epoch_within_the_reads_deadline`, `test_a_slot_bound_to_a_term_it_cannot_hold_is_refused_naming_the_term`
- `types/read_route_tests.rs`: `test_a_read_of_a_bound_subject_reads_its_owner_alone`, `test_a_snapshot_read_of_an_unbound_subject_reads_every_holder`, `test_a_latest_read_reaching_two_partitions_is_refused_with_code_25`, `test_a_latest_read_bound_to_one_partition_reaching_another_is_refused`, `test_a_read_write_read_of_its_own_partition_is_answered`, `test_a_read_write_read_reaching_another_partition_is_refused_with_code_23`, `test_a_latest_reverse_read_reaching_a_referrers_partition_is_refused`
- `types/read_times_tests.rs`: `test_a_read_choosing_its_version_holds_none_while_it_is_short`, `test_a_read_choosing_its_version_holds_it_once_it_lasts`, `test_a_held_version_takes_a_frame_every_third_of_the_stall`
- `types/result_sink_tests.rs`: `test_a_short_read_is_one_last_batch`, `test_rows_past_a_frame_go_in_the_next_batch`, `test_a_row_too_large_for_any_frame_refuses_the_read`, `test_a_row_whose_terms_share_a_string_fits_by_its_frame`, `test_a_resume_key_not_fitting_beside_the_rows_ends_the_read_alone`, `test_a_resume_key_too_large_for_any_frame_refuses_the_read`, `test_a_read_holds_its_batches_to_its_clients_credit`, `test_a_batch_fills_to_exactly_the_clients_frame`, `test_a_resume_key_fills_its_last_batch_to_exactly_the_clients_frame`, `test_a_row_past_the_largest_frame_refuses_the_read`, `test_a_read_waiting_for_credit_ends_with_its_connection`
- `types/resume_terms_tests.rs`: `test_a_read_resumed_goes_on_after_the_keys_row`, `test_a_resume_key_of_an_unheld_term_is_refused`, `test_a_resume_key_of_a_term_no_row_names_is_refused`
- `types/row_terms_tests.rs`: `test_a_skolem_subject_comes_back_a_blank_node`, `test_a_reifier_comes_back_a_blank_node`, `test_a_datatype_held_by_id_comes_back_by_its_iris_hash`, `test_a_resume_key_names_a_literal_by_its_lexical_form`, `test_a_read_resumed_at_a_tagged_or_typed_literal_goes_on_after_it`
- `types/scan_stream_tests.rs`: `test_a_partitions_refusal_comes_back_as_it_was_sent`, `test_a_scan_given_up_before_its_end_is_cancelled`, `test_a_read_given_up_on_a_silent_partition_closes_its_connection`, `test_a_partition_silent_past_a_scans_wait_refuses_the_read`, `test_a_scan_waiting_when_its_connection_closes_is_refused`
- `types/wanted_names_tests.rs`: `test_a_page_naming_more_terms_than_a_scan_holds_reads_them_in_several`
- `utils/literals_tests.rs`: `test_a_plain_literal_object_reads_its_reverse_list`, `test_a_language_tagged_literal_object_reads_its_rows`, `test_a_literal_of_a_known_datatype_reads_its_rows`, `test_a_literal_of_a_datatype_named_by_id_reads_its_rows`, `test_a_vector_object_reads_the_forward_lists`, `test_a_literal_of_a_datatype_the_partition_does_not_name_answers_no_row`
- `utils/refusals_tests.rs`: `test_a_cross_partition_read_names_both_partitions_in_its_detail`
- `utils/run_read_tests.rs`: `test_a_snapshot_read_names_its_epoch_and_holds_its_version`, `test_a_read_write_or_named_read_names_epoch_zero`, `test_a_limited_read_resumes_after_its_last_row`, `test_a_limited_read_names_no_row_past_its_limit`, `test_a_read_over_two_partitions_merges_in_key_order_and_resumes`, `test_a_monotonic_fast_read_over_partitions_reads_at_their_lowest_close`, `test_a_reverse_read_asks_the_partitions_its_referrer_set_names`, `test_a_monotonic_fast_read_leaves_no_stream_on_its_links`, `test_a_resume_key_of_another_pattern_or_version_is_refused`, `test_a_resume_key_naming_an_unheld_term_past_the_frame_is_refused_within_it`, `test_a_closed_failed_refuses_a_monotonic_fast_read`, `test_a_reverse_read_named_or_monotonic_fast_reads_its_members`

The property target's executor laws and `bdd_executor`'s steps drove `ReadExecutor` directly before this task and are
unchanged.

## Removed

Each test whose only subject was the role session, with where its law is held now. No new test was needed: each law was
held over `ReadExecutor` already.

| Test | Its law | Held by |
|---|---|---|
| `types::compute_session_tests::test_a_read_larger_than_a_frame_is_answered_batch_after_batch` | four objects of a third of the floor frame each, read at `max_frame` the floor, answer every row in at least two batches, each sent once the one before has left and the stream holds credit | `types::read_executor_tests::test_no_batch_is_built_before_its_room`: the same four objects and frame, every batch within it, no batch before its room, the same rows in at least two batches; `bdd_executor`'s "A batch stops at the client's frame"; the property law L5.7, `test_no_batch_is_handed_out_past_its_room` |
| `types::compute_session_tests::test_a_cancelled_read_is_dropped_at_once` | a read cancelled while its partition is silent is open no longer at once, so a stop does not wait on it | `types::read_executor_tests::test_an_executor_with_open_counts_each_read_until_it_ends`: the same silent Scan, the read counted open, and none once cancelled; `types::read_executor_tests::test_a_cancelled_read_hands_out_nothing_more`; `bdd_executor`'s "A Cancel stops a read in the turn it is read" |
| `property::types::read_executor_property_tests::test_both_callers_get_one_answer` | L5.14: a read answers the same frames through the session's seam in process and as a ComputeRead through `ComputeSession` | none needed: `notes/laws.md` states that L5.14 holds while both callers exist and goes with `ComputeSession`'s role session in task 6.4. The one caller left, a client's read in process, is held by L5.1, L5.2, L5.4, L5.6 and L5.7 in the same file |

Removed by `git rm`, every test in it removed as above:
`ultragraphdb_common/ultragraphdb_common_compute/tests/types/compute_session_tests.rs`, with its line in
`tests/types/mod.rs`.

## The gate

- `bazel build //...` passes, with no warning.
- The Clippy aspect over `//ultragraphdb_common/ultragraphdb_common_compute/...` is clean. Its one finding,
  `large_enum_variant` on the session's asks, was fixed by boxing the `ReadRequest` a start carries.
- `rustfmt --check` over every changed file and the module files above them is clean.
- `bazel test //ultragraphdb_common/ultragraphdb_common_compute/...` passes: 13 of 13 targets.
- One intermittent failure, recorded: `bdd_executor`'s "A partition answering within the bound is not hung" failed once
  with code 17 ("partition 1 sent no frame of a Scan within 200ms") at a load average of 21, and passed on its one
  rerun. The scenario and its steps are unchanged by this task, and its margin is 50 ms: frames 150 ms apart against
  twice a stall of 100 ms.

## Found, not changed

- `tests/endpoint_stage.rs`' `view_frame` is documented as reading "a ComputeRead `frame` the double took". It validates
  any frame; the comment is stale and names no session.
- `tests/apart/` keeps its name, though compute no longer runs on a thread apart from the test's: renaming it would touch
  every file that includes it, for no change of behaviour.
- `src/types/read_request.rs`' `ReadRequest::of` over a `ComputeReadView` is reached only by `ComputeSession`, and goes
  with it.
