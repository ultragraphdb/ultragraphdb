# Group 4: one connection per core and partition

## Checkpoint A: tasks 4.1 and 4.2, phases 1 to 3 (2026-11-19, base `ad272ccf`)

### The specification first

Checkpoint C of group 6 recorded `ultragraphdb_compute_tests:bdd_quad_reads_compute_reads` refused with "the connection
to the system group is closed". Its root cause, found apart: compute's `/ready` followed core 0's `SystemLink` alone, while
every core accepts clients from one shared queue and relays their SCRAM exchange over its own link, so a login landing on a
core whose link had not yet subscribed was answered 17. The user decided that compute is ready only once every endpoint
core's link holds. Before any test:

- role-binaries delta, "Pinning…": compute is "ready only once every runtime is", where it said "its readiness on the
  first".
- role-binaries delta, "Readiness, health and shutdown": compute is ready once, on every runtime, the group answers that
  runtime's connection, its view holds a placement and a close, Members lists every partition and a partition answers;
  `/ready` answers 503 from the moment any runtime's connection closes until that runtime holds again. A scenario was
  added, "Compute is ready only once every runtime's link holds".
- `design.md` D14 records the decision and its alternative. No client-endpoint or system-group requirement said the first
  core; `service.rs`' doc ("ready once its first core's view holds…") is restated in phase 4 with the code it describes.

### Phase 1: the surface

| Item | Where | Visibility |
|---|---|---|
| `ComputeCore::over(links: RoleLinks, group: Address, times: ReadTimes)`: a core whose reads travel on its links (D2) | `common_compute`, `compute_core/mod.rs` | pub |
| `ComputeCore::links_held()`: the links to partitions held and how many hold a stream | same | pub, `test-utils` |
| `SystemLink::watchers_held()`, `SystemLink::outcomes_held()` | `system_link/mod.rs` | pub, `test-utils` |
| `RoleLink::open_read`, `grant`, `streams_open`: a read's stream on the link, its frames whole and unread, its credit | `role_link/mod.rs` | `pub(crate)` |
| `Readiness::follow(&self, parts: &[Readiness])`: ready exactly while every part is | `common_service`, `readiness/mod.rs` | pub |

Every body is `unimplemented!()`. The phase-1 surface passes `make build`'s targets and the Clippy aspect over the four
packages it touches (`common_compute`, `common_service`, `ultragraphdb_compute`, `ultragraphdb_compute_tests`), so
checkpoint A can be committed on its own.

### Phase 2: the suite

Harness, written in this phase:

- `LinkProbe` and `Told` (`src/utils_test/link_probe.rs`, `test-utils`): one core's link to one partition driven as the
  core's sessions and reads drive it, and what each stream was told. `RoleDouble::accepted()` counts the connections a
  double took.
- `tests/apart/apart_tap.rs`: the tap in front of a played partition, following each Scan with a Credit compute never sent
  (`overgrant`), so the past-credit refusal is pinned in the crate's own suite (feasibility §4, case 3).
- `tests/apart/apart_link.rs`: the probe against a partition double holding every request, behind the tap when asked.
- `ApartCluster::start_in(order, tapped)`: partitions bound to their ports in a drawn order and listed by number
  (feasibility §4, case 5); `links`, `links_within`, `core_over`, `session_on`, `tap`, `role_cluster`;
  `links_listing` builds a core's links as its system link learns them. The played partition answers a Bind and a Write and
  records each request by its connection (`requests`).
- Every harness building a core builds it over links: `ApartCluster`, `utils/played_cluster`, `played_reads`,
  `played/PlayedCluster`, `endpoint_stage`, `client_session_tests`. So L5.1 to L5.13 and every executor and endpoint test
  run with the connection changed beneath them, their texts unchanged. `PlayedCluster::start_own` and `start_placed_own`
  keep `ComputeCore::new` for the tests of the core's own connections and asks, which task 4.3 removes.
- `ultragraphdb_compute_tests/tests/gate.rs`: a gate in front of the system group, carrying compute's `i`th connection after
  `i` × 300 ms, run on the scenario's runtime, so it needs no thread of its own.

New tests:

| What | File | Count |
|---|---|---|
| C4.1 to C4.22 | `tests/types/role_link/role_link_tests.rs` | 20 |
| L4.1 to L4.4 | `tests/property/types/role_link_property_tests.rs` | 4 |
| The watchers and the dropped token (task 4.2) | `tests/types/system_link_tests.rs` | 2 |
| "A transaction's reads share its writes' core" | `features/quad-reads/cores.feature`, `tests/cores_steps.rs`, target `bdd_executor` | 1 scenario |
| `Readiness::follow`: one core unready, all ready, one withdrawn, none | `common_service`, `readiness_tests.rs` | 3 |
| "Compute is ready only once every runtime's link holds" | `compute_tests`, `features/role-binaries/unready.feature`, `system_steps.rs` | 1 scenario |

The corner cases, by test of `role_link_tests.rs`:

| Case | Test |
|---|---|
| C4.1 | `test_a_scan_that_cannot_be_built_registers_no_stream`; its stray frame after: `link_tests`' `test_a_frame_of_a_stream_whose_holder_is_gone_is_checked_whole`, rebound |
| C4.2 | `test_a_scan_on_a_link_closed_beneath_it_registers_no_stream` |
| C4.3, C4.4 | `test_a_failed_write_closes_the_link_and_the_next_request_dials_again` |
| C4.5 | `test_a_frame_past_credit_closes_the_link_and_ends_every_read` |
| C4.6, C4.17 | `test_entries_up_to_the_credit_keep_the_link` |
| C4.7 | `test_a_frame_of_a_gone_reader_is_checked_whole` |
| C4.8 | `test_each_scan_reaches_its_partition_whatever_the_order_bound` |
| C4.9 | `test_entries_on_a_bind_stream_close_the_link` |
| C4.10 | `test_a_held_on_a_scans_stream_closes_the_link` (Neighbours and Frontier: see the open questions) |
| C4.11 | `test_a_frame_ahead_of_the_streams_closes_the_link` |
| C4.12 | `test_an_error_ends_its_read_and_one_on_stream_0_closes_the_link` |
| C4.13 | `test_entries_after_a_cancel_are_late` |
| C4.14 | `test_a_scan_past_credit_ends_a_commit_on_the_same_link` |
| C4.15 | `test_a_hung_partition_ends_a_bind_stream_on_the_same_link` |
| C4.16 | `test_a_transactions_bind_and_reads_share_one_link`, and the scenario of `cores.feature` |
| C4.18 | `test_a_closed_request_is_answered_on_its_stream` |
| C4.19 | `test_a_dial_past_its_bound_closes_the_link` |
| C4.20 | `test_one_cores_link_closing_leaves_the_others` |
| C4.21 | `test_a_storage_process_restarted_under_a_read_is_dialled_again` |
| C4.22 | `test_a_monotonic_fast_read_asks_closed_on_each_link_before_its_scans` |

The laws: L4.1 `test_the_link_agrees_with_its_model` (a drawn sequence against a model of the link's streams), L4.2
`test_frames_past_the_credit_close_the_link`, L4.3 `test_a_frame_without_a_reader_is_checked_whole`, L4.4
`test_a_read_reaches_its_partitions_whatever_the_order_bound`.

### Phase 2: the failing run (2026-11-19)

| Target | Run | Failed | Why |
|---|---|---|---|
| `common_compute` unit | 322 | 183 | `not implemented`: `ComputeCore::over` 167, `RoleLink::open_read` 14, `SystemLink::watchers_held` 1, `outcomes_held` 1 |
| `common_compute` property | 21 | 8 | `not implemented`: `ComputeCore::over` 5, `RoleLink::open_read` 2, `RoleLink::grant` 1 |
| `common_compute` BDD, 11 targets | 148 scenarios | 130 | each a step panicking `not implemented`; `bdd_system_link` (15) and 3 of `bdd_catalog` reach no read |
| `common_service` unit | 77 | 3 | `not implemented`: `Readiness::follow` |
| `compute_tests:bdd_role_binaries_unready` | 2 scenarios | 1 | the new scenario, for the stated reason: 63 logins refused `(0, Unavailable)` once `/ready` answered |

Every failure is the unimplemented panic of a phase-1 body, but the readiness scenario's, which fails by its assertion
against the unchanged binary, as the coordinator's brief asked. The 139 unit tests that pass reach no new surface.

### Phase 3: the inputs against the corner cases

The suite's inputs were read for cases where two distinct quantities coincide:

- **Bind order and partition number.** C4.8 binds 3, 1, 2; L4.4 draws every permutation of 2 to 4, the identity among
  them but not alone.
- **A read's end and the link's close.** C4.12's Error carries code 24 (TimedOut), not 17, so a read ended by its Error is
  told apart from one ended by the close.
- **A late frame that is never sent.** C4.7, C4.13 and L4.3 carry the late frame on another open stream of the double's,
  since a partition sends nothing on a stream compute cancelled: without the carrier the late frame could be dropped at
  the partition and the test would pass on nothing.
- **Credit.** C4.6 sends exactly the credit, then one more after one grant; L4.2 draws grants 0 to 3 and frames from 1 to
  two past the credit, so the boundary and both sides of it occur; the other read on the link is a different stream.
- **The link and the core.** C4.20 runs two cores against one partition: the partition's connection count (2, then 3)
  tells one link per core from one per process.
- **Ordering on one link.** C4.16 and the scenario check the Write before the Scan on one connection and the Scan naming
  the Bind's transaction; C4.22's read epoch (1) is not the played epoch (9).
- **What is not covered as the enumeration states it:** Frontier and Neighbours exist in `PROTOCOL.md` alone, so C4.10's
  last two inputs and L4.1's Frontier requests are not drawn (open question 3). L4.3 breaks one rule of §2.5 steps 4 to
  15, the chunk's checksum, not each. C4.21 observes the Members read as the closed link found
  (`RoleLinks::closed_found`), since its core has no system link; the binary's scenario "A storage process restarted at
  its address is dialled again" holds the rest.

### Open questions

1. **Task 4.3's removals, before phase 4.** Once the binary builds its cores with `ComputeCore::over`, the old path
   (`ComputeCore::new` and its connections, `Link`, `ScanStream`'s use of it) is reached only by the tests task 4.3
   removes. Kept until checkpoint C, phase 4 must carry two carriers beneath `ScanStream`, `PartitionRows`, `PinTarget`,
   the lookups and `first_at`, which task 4.3 then deletes. Recommendation: approve task 4.3 now, so phase 4 and the
   removals land as one commit, as tasks 5.6 and 5.7 did. The list is under checkpoint C below, to be finished there.
2. **The placement map.** `ComputeCore::over` keeps reading Placement at the group itself; only Members and the
   partitions' connections move to the core's links. Routing by the links' map would change what a read routes by under
   a stale map, which `routing.feature`'s `older_map` scenarios pin. Recommendation: keep it so in group 4.
3. **Frontier and Neighbours.** Neither command is in `ultragraphdb_protocol_wire`. `RoleLink` records every request by
   its command (`Command::answered_by`), so a Frontier is carried once the wire has it. Recommendation: no group-4 code for
   them; C4.10's two inputs and L4.1's Frontier move to the stage that adds the commands.
4. **`ultragraphdb_common_service`.** `Readiness::follow` lives there, since a binary holds no unit test. A small edit to
   a crate outside group 4's list.

### Checkpoint C, prepared

What task 4.3 removes, with the test over `RoleLink` that holds each removed test's law:

| Removed test | Law held by |
|---|---|
| `compute_core_tests`: `test_the_reads_of_a_core_share_its_connection_to_a_partition` | `role_link_tests`' `test_a_transactions_bind_and_reads_share_one_link`, `link_tests`' `test_a_link_kept_holds_no_stream_of_a_lookup_returned` (rebound) |
| `compute_core_tests`: `test_a_system_group_silent_past_the_bound_refuses_the_read` | its placement half by `test_a_placement_other_than_the_even_map_refuses_the_read` over links; its Members half has no ask left to hang |
| `compute_core_tests`: `test_a_partition_not_reached_refuses_the_read` | `test_a_dial_past_its_bound_closes_the_link`, `test_a_storage_process_restarted_under_a_read_is_dialled_again` |
| `compute_core_tests`: `test_reads_arriving_during_a_dial_share_it` | `test_a_failed_write_closes_the_link_and_the_next_request_dials_again` (one dial per link) |
| `compute_core_tests`: `test_members_listing_compute_processes_reach_the_partition` | `role_links_tests`' Members tests: `RoleLinks::learned` lists storage alone |
| `compute_core_tests`: `test_the_placement_and_the_members_are_read_at_the_group` | its placement half stays a law of `over`; Members is the system link's (`system_link_tests`' `test_members_are_read_into_the_links`) |
| `dial_under_way_tests` (2) | none needed: no dial is shared across reads' tasks once `RoleLinks` dials |

Code: `src/types/link.rs`; `ComputeCore`'s `links`, `dialling`, `addresses`, `address_of`, `fetch_members`, `link`'s dial
path, `dial_for_all`, `dial`, `DialUnderWay` (`compute_core/dial_under_way.rs`), and `ComputeCore::new`;
`PlayedCluster::start_own` and `start_placed_own`. To be confirmed against phase 4's code at checkpoint C.
