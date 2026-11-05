# Group 5: placement and the sequencer

Three new crates and a new group folder, `ultragraphdb_cluster/`:

- `ultragraphdb_error/ultragraphdb_error_cluster`: `ClusterError(pub ClusterErrorEnum)`, its own crate because the
  placement map, the sequencer and, from groups 6 and 7, the metadata group and the partitions raise it. It converts
  from `LogError` and `InvariantError`.
- `ultragraphdb_cluster/ultragraphdb_cluster_placement`: `PartitionId`, `NodeId`, `BucketRange`, `NamespaceRun`,
  `BucketGroup`, `PlacementMap` with its checks, lookups at an epoch, preferred leaders and encoding.
- `ultragraphdb_cluster/ultragraphdb_cluster_sequencer`: `Sequencer<L: OrderedLog>`, `SequencerConfig`,
  `ClockReading`, `Close`, `CatalogVersion`, `SequencedTransaction`, `SequencerRecord`.

Two small additions to `ultragraphdb_core_ids`, test first: `Namespace::bucket_bits` (the top byte of XXH3-64 of the
tenant's 8 big-endian bytes, which `Bucket::derive` now calls, so the hash rule has one owner and the placement map
names a tenant's run by it) and `Epoch::ZERO` (epoch 0, which a fresh sequencer and a map's first layout need
without a fallible constructor).

Root `Cargo.toml` (member glob `ultragraphdb_cluster/*`, three workspace dependencies), `Cargo.lock`, `AGENTS.md`
(the group folder, three crate rows, the `core_ids` row), `fuzz/Cargo.toml`, `fuzz/Cargo.lock` and two fuzz packages.

## Phases

### 1. Surface

Every type, constant, constructor, method, error variant and its constructor, `Display`, `Error::source` and `From`,
with bare `unimplemented!()` bodies and no `#[expect]`; the constants are values. The three libraries built under
Bazel (`--noincompatible_disallow_empty_glob` for that one build only, since the test globs had no files yet).

### 2. Suite

- Scenarios, verbatim: `ultragraphdb_cluster_sequencer/features/sequencer/sequencer.feature` (all 19 scenarios of the
  six requirements of `specs/sequencer/spec.md`) and
  `ultragraphdb_cluster_placement/features/metadata-group/placement_map.feature` (the 4 scenarios of "The placement map
  assigns bucket ranges to partitions"), each bound by its crate's `bdd` target (cucumber, `fail_on_skipped`).
- Unit tests mirroring `src/`, rooted at `tests/mod.rs`. The placement map is checked against a plain reference model
  (`tests/utils/reference_model.rs`: each bucket answered by a linear scan over the assignments, no sort and no binary
  search; the first faulty bucket found by counting coverage at bucket 0 and at every range's start and end, the only
  buckets where coverage changes), over 200 seeded random layouts with one of three faults, at every cut and at random
  buckets, and bucket by bucket over a whole tenant run.
- The sequencer runs over the real write-ahead log on its simulated storage (`tests/utils/harness.rs`, the log in
  `/seq`, segments of 64 blocks). The scenarios of "Partitions learn each closed epoch and stamp the open one" are driven
  by the sequencer's deliveries into a model of a partition (`tests/utils/partition_model.rs`, the BDD target's alone).
- Expected values are hand-written: the encodings of `PlacementMap` and `SequencerRecord` byte by byte, the even
  split's sizes (21,846, 21,845, 21,845; 64 each at 1,024 partitions), group and run arithmetic, texts.
- Corner cases enumerated before the suite, each with its test: partition 0, 1, `u32::MAX`; ranges `0..1`,
  `(2^24 − 1)..2^24`, `0..2^24`, empty, reversed, past `2^24`; groups 0, `2^18 − 1`, `2^18`; runs 0 and 255; an even map
  over 0, 1, 2, 3, 1,024 and 1,025 partitions; a change at, below and above the latest epoch, a gap at 0, in the middle
  and at the end, an overlap at 0, in the middle, at the end, a gap before an overlap and an overlap before a gap, an
  empty layout, 1,024 and 1,025 partitions, `2^18` and `2^18 + 1` ranges, 256 and 257 layouts; lookups at the edges of
  every range and at epochs before, at and after a change and at `2^42 − 1`; a group in one range, across a boundary,
  with a partition repeated; settings at 0, 1, 1,000, 1,001 and `epoch_ms_min − 1`, `epoch_ms_min`, 10,000, 10,001;
  ticks before, at and long after a deadline; requests for a closed, the open and a later epoch; the clock stepping back
  once and twice; closes with 256 and 257 versions, 4,096 and 4,097 evictions; transactions with 0, 1,024 and 1,025
  partitions, 65,536 and 65,537 body bytes, a reply epoch equal to and one past the epoch; the last epoch `2^42 − 1`;
  1,024 and 1,025 subscribers; a subscriber behind the retained closes; `as_of` before, at, between and after closes,
  at two closes of one time, with no close and with an undurable close; every truncation of every encoding.
- Every error variant is provoked through a public API and asserted by variant, except `Invariant`, reachable only
  through a bug (an invariant panics in a debug build), which the error crate's own tests construct.

Run against the surface (`bazel test //ultragraphdb_error/ultragraphdb_error_cluster/... //ultragraphdb_cluster/...`):

| Target | Tests | Failed | Reason |
|---|---|---|---|
| `error_cluster` unit | 12 | 12 | `not implemented` panic, all 12 |
| `cluster_placement` unit | 57 | 53 | `not implemented` panic, all 53; the 4 passing assert constants (`BucketGroup::BUCKETS`/`COUNT`, `BucketRange::END`, `NamespaceRun::BUCKETS`/`GROUPS`, the map's limits), values of the surface |
| `cluster_placement` bdd | 4 scenarios | 4 | `Step panicked. Captured output: not implemented`, 4 of 4 |
| `cluster_sequencer` unit | 94 | 91 | `not implemented` panic, all 91; the 3 passing assert constants |
| `cluster_sequencer` bdd | 19 scenarios | 19 | `Step panicked. Captured output: not implemented`, 19 of 19 |
| `core_ids` unit (the two new tests) | 62 | 2 | `not implemented` in `Namespace::bucket_bits` |

The first run had one failure that was not the panic: the harness put the log in `/data/seq`, whose parent the
simulated storage does not create. The directory became `/seq`; the rerun failed 91 of 94, every one the panic.

### 3. Audit

Each defect planted alone in the implementation, the crate's Bazel targets run, the defect reverted (the script and
its results in the session scratchpad, `audit/`). 64 defects; every one is caught by at least one test whose subject is
the defective behaviour. The first run left two defects alive, P23 (a repeated leader decoded: the test had leaders out
of order but none repeated) and P26 (the range limit refusing the limit itself: `even(1024)` builds its layout without
the check); `test_leaders_are_checked` gained a repeated leader and `test_a_change_past_the_most_ranges_is_refused` a
change of exactly `2^18` ranges, and both were caught on the rerun. S18 first failed to compile (removing the check left
the set's type uninferred) and was re-planted as a check that never fires.

| Defect | Caught by (tests, scenarios) |
|---|---|
| E01 a text's fields swapped | `test_sequencer_texts_name_the_values` |
| E02 a carried log error not the source | `test_a_carried_log_error_is_the_source` |
| E03 a constructor building another variant | `test_placement_texts_name_the_values`, `test_each_constructor_builds_its_variant`, `test_errors_of_one_variant_differ_by_their_fields` |
| K01 bucket bits from the hash's low byte | `test_the_scenario_bucket_of_http_ex_org_a`, `test_the_golden_buckets_are_the_reference_hashes_top_bits`, `test_every_golden_bucket_matches_the_reference`, `test_bucket_bits_are_the_reference_hashes_top_byte` and 4 more |
| P01 partition 0 read as 1 | `test_partition_zero_is_refused`, `test_leaders_are_checked`, `test_ranges_are_checked` |
| P02 a range end bound off by one | `test_a_range_past_the_last_bucket_is_refused` |
| P03 an empty range taken | `test_an_empty_or_reversed_range_is_refused` |
| P04 contains takes the end | `test_a_range_contains_its_buckets_alone` |
| P05 a bucket's run read from the wrong byte | `test_the_first_and_the_last_runs`, `test_a_namespaces_run_is_its_derived_buckets_top_bits` |
| P06 a bucket's group shifted by 7 | `test_a_buckets_group_is_its_top_18_bits`, A bucket group resolves at an epoch (scenario) |
| P07 the group bound off by one | `test_a_group_past_18_bits_is_refused` |
| P08 a group's run shifted by 9 | `test_a_groups_range_and_run` |
| P09 the larger ranges given to the last partitions | `test_a_bucket_resolves_at_the_edges_of_its_ranges`, `test_a_change_applies_from_its_epoch`, `test_three_partitions_split_every_run_in_order`, A bucket's partition is found (scenario) |
| P10 the partition bound off by one | `test_an_even_map_over_no_partition_or_too_many_is_refused` |
| P11 a change at the latest epoch taken | `test_a_change_not_after_the_latest_is_refused` |
| P12 the gap check removed | `test_a_gap_before_an_overlap_is_named_first`, `test_a_gap_is_refused_naming_its_first_bucket`, `test_changes_agree_with_the_reference_model`, A change leaving a bucket unassigned is refused (scenario) |
| P13 a gap named by the range after it | `test_a_gap_before_an_overlap_is_named_first`, `test_a_gap_is_refused_naming_its_first_bucket`, `test_changes_agree_with_the_reference_model`, A change leaving a bucket unassigned is refused (scenario) |
| P14 the final coverage check removed | `test_a_gap_is_refused_naming_its_first_bucket`, `test_changes_agree_with_the_reference_model` |
| P15 runs not merged | `test_encoding_appends_to_the_buffer`, `test_maps_encode_as_written_by_hand`, `test_the_hand_written_encodings_decode`, `test_a_change_is_normalised_whatever_its_order_and_cuts` and 1 more |
| P16 leaders of emptied partitions kept | `test_a_change_drops_the_leader_of_a_partition_it_empties_and_adds_none` |
| P17 a layout applying one epoch late | `test_a_bucket_resolves_by_the_layout_in_effect_at_the_epoch`, `test_a_group_names_each_partition_once_in_order`, `test_a_partition_repeated_within_a_group_is_named_once`, `test_ranges_at_covers_every_bucket_once_in_order` and 4 more |
| P18 a bucket at a run start given the run before | `test_a_bucket_resolves_at_the_edges_of_its_ranges`, `test_a_bucket_resolves_by_the_layout_in_effect_at_the_epoch`, `test_every_bucket_of_a_run_matches_the_reference_model`, `test_changes_agree_with_the_reference_model` and 1 more |
| P19 group partitions not deduplicated | `test_a_partition_repeated_within_a_group_is_named_once` |
| P20 a group takes the run after it | `test_a_group_in_one_range_resolves_to_its_partition`, A bucket group resolves at an epoch (scenario) |
| P21 a leader's partition and node swapped | `test_encoding_appends_to_the_buffer`, `test_maps_encode_as_written_by_hand`, `test_a_map_round_trips` |
| P22 neighbours of one partition decoded | `test_ranges_are_checked` |
| P23 a repeated leader decoded | `test_leaders_are_checked` |
| P24 trailing bytes accepted | `test_bytes_after_the_end_are_refused` |
| P25 one layout too many kept | `test_the_map_keeps_at_most_256_layouts` |
| P26 the range limit refusing the limit | `test_a_change_past_the_most_ranges_is_refused` |
| P27 a first layout from epoch 1 decoded | `test_layout_epochs_are_checked` |
| S01 epoch_ms_min bound off by one | `test_epoch_ms_min_above_1000_is_refused`, `test_epoch_ms_min_is_checked_first`, Epoch lengths out of their bounds stop the start (scenario) |
| S02 epoch_ms_max equal to epoch_ms_min refused | `test_a_close_not_yet_durable_is_not_mapped`, `test_a_time_before_the_first_close_is_refused`, `test_a_time_maps_to_the_latest_close_at_or_before_it`, `test_no_time_maps_before_any_close` and 20 more |
| S03 a request ignored | `test_a_request_after_epoch_ms_min_closes_at_once`, `test_a_request_closes_once_epoch_ms_min_has_passed`, A partition's request closes the epoch early (scenario), A waiting read closes the epoch early (scenario) |
| S04 close times follow a clock stepping back | `test_a_clock_stepping_back_records_the_last_closes_time`, `test_a_close_time_never_falls_below_the_recovered_last`, A clock stepping back does not lower close times (scenario) |
| S05 a request outliving its close | `test_a_request_closes_once_epoch_ms_min_has_passed` |
| S06 closes spaced from the start | `test_a_late_tick_closes_once_and_counts_from_itself`, `test_a_request_closes_once_epoch_ms_min_has_passed`, `test_a_request_for_a_closed_epoch_changes_nothing`, `test_no_close_before_the_deadline` and 3 more |
| S07 a request for the open epoch refused | `test_a_request_after_epoch_ms_min_closes_at_once`, `test_a_request_closes_once_epoch_ms_min_has_passed`, A partition's request closes the epoch early (scenario), A waiting read closes the epoch early (scenario) |
| S08 a request for a closed epoch closing early | `test_a_request_for_a_closed_epoch_changes_nothing` |
| S09 closes never counted durable | `test_the_oldest_close_held_bounds_the_mapping`, `test_a_time_maps_to_the_latest_close_at_or_before_it`, `test_a_close_not_yet_durable_is_not_mapped`, `test_a_time_before_the_first_close_is_refused` and 32 more |
| S10 closes delivered before durable | `test_a_close_counts_only_once_synced`, `test_a_failed_sync_is_the_logs_error_and_nothing_is_delivered`, An unacknowledged close is not delivered (scenario) |
| S11 a trimmed catch-up not refused | `test_a_subscriber_behind_the_held_closes_is_refused` |
| S12 a time equal to a close mapped before it | `test_a_time_maps_to_the_latest_close_at_or_before_it`, `test_a_close_not_yet_durable_is_not_mapped`, `test_recovery_holds_only_the_retained_closes`, `test_recovery_rebuilds_the_closes_and_transactions` |
| S13 closes not durable mapped | `test_no_time_maps_before_any_close`, `test_a_close_not_yet_durable_is_not_mapped` |
| S14 one transaction past the capacity held | `test_the_sequencer_holds_at_most_its_retained_transactions` |
| S15 transactions not durable read | `test_a_transaction_is_read_back_only_once_durable` |
| S16 the epoch read from left out | `test_a_log_may_begin_at_any_close`, `test_reading_from_an_epoch_skips_the_earlier_ones`, Transactions keep their order and epoch (scenario) |
| S17 the catalog limit refusing the limit | `test_the_most_versions_and_evictions_are_taken`, `test_records_at_their_limits_round_trip` |
| S18 a partition named twice taken | `test_the_first_partition_named_twice_is_named`, `test_the_checks_run_in_order`, `test_a_refused_transaction_appends_nothing`, `test_a_decoded_transaction_is_checked_as_a_built_one` |
| S19 a reply epoch equal to the epoch refused | `test_a_reply_epoch_equal_to_its_epoch_is_taken`, `test_a_log_may_begin_at_any_close`, `test_a_transaction_outside_the_open_epoch_diverges`, `test_more_transactions_than_retained_refuse_the_recovery` and 7 more |
| S20 a close's epoch and time swapped | `test_the_last_epoch_refuses_the_next_close`, `test_a_kill_after_every_synced_append_keeps_it`, `test_a_kill_after_every_append_before_its_sync_loses_only_it`, `test_a_close_time_never_falls_below_the_recovered_last` and 16 more |
| S21 partition 0 decoded as 1 | `test_partition_zero_is_refused` |
| S22 trailing bytes accepted | `test_bytes_after_the_end_are_refused` |
| S23 a round's version not checked | `test_a_rounds_version_other_than_its_epoch_diverges` |
| S24 a close out of order replayed | `test_a_close_not_one_after_the_last_diverges` |
| S25 a falling close time replayed | `test_a_close_time_below_the_last_diverges` |
| S26 replayed transactions not durable | `test_a_log_may_begin_at_any_close`, `test_a_kill_after_every_synced_append_keeps_it`, `test_a_kill_after_every_append_before_its_sync_loses_only_it`, `test_recovery_rebuilds_the_closes_and_transactions` and 2 more |
| S27 the last round not replayed | `test_the_last_epoch_refuses_the_next_close`, `test_a_kill_after_every_synced_append_keeps_it`, `test_a_kill_after_every_append_before_its_sync_loses_only_it`, `test_a_close_time_never_falls_below_the_recovered_last` and 7 more |
| S28 subscribers unbounded | `test_at_most_1024_partitions_subscribe` |
| S29 the first close after a start timed from 0 | `test_an_empty_log_starts_a_fresh_sequencer`, `test_a_recovered_sequencer_continues_the_epochs`, `test_the_first_close_is_due_epoch_ms_max_after_the_start` |
| S30 versions effective at the closed epoch | `test_announced_versions_take_effect_one_epoch_later`, `test_no_epoch_follows_the_last_close`, `test_a_close_carries_the_pending_versions_and_evictions`, A version reaches every partition before its epoch (scenario) and 1 more |
| S31 one close more than retained | `test_the_oldest_close_held_bounds_the_mapping`, `test_a_subscriber_behind_the_held_closes_is_refused`, `test_recovery_holds_only_the_retained_closes` |
| S32 recovery past the retained transactions | `test_more_transactions_than_retained_refuse_the_recovery` |
| S33 a subscription one past the last close taken | `test_a_subscription_past_the_last_durable_close_is_refused` |

Input variety: the scenario values coincide nowhere the suite relies on (the even split's three sizes differ from the
partition numbers; the test of a change moves a range inside partition 2's, away from its boundaries; close times and
monotonic times differ in every test of the clock stepping back, of `as_of` and of recovery). The one coincidence, the
harness's clock `at(ms)` giving equal monotonic and wall times, is broken by every test where the two must differ.

### 4. Implementation

- `PlacementMap` holds its first layout (from epoch 0) and the later ones apart, so a map always has a layout; a
  layout is runs of (first bucket, partition), normalised, with the set of its partitions derived once at build. A
  change is sorted and swept once: before the first fault the ranges tile `0..expected` exactly, so the first fault is
  at `expected` (a gap) or at a range's start (an overlap).
- The sequencer appends one round per record, its version the record's epoch, its transaction count 0 for a close and
  1 for a transaction. `hold` keeps a record after its append and checks, as invariants, what replay checks as
  refusals: a close one after the last, at or after its time; a transaction in the epoch after the last close appended.
  The last durable close is its own field, set by a successful sync, because trimming the retained closes would
  otherwise lose it; closes are trimmed only after a sync, when every close held is durable.
- Recovery reads every round from position 1 to the committed position, 1 MiB per read, bounded by the rounds.

Under Bazel: `error_cluster` unit 12 passed; `cluster_placement` unit 58 passed, bdd 4 scenarios (8 steps) passed;
`cluster_sequencer` unit 95 passed, bdd 19 scenarios (38 steps) passed; `core_ids` unit 63 passed, bdd 9 scenarios
passed. One expectation was corrected during the implementation: `test_a_group_names_each_partition_once_in_order`
expected group 2 at epoch 7 to resolve to partitions 1 and 4, but buckets 128 and 129 lie in `100..130`, on partition 2;
the hand computation was wrong, not the specification, and the expected value is now 1, 2 and 4.

Coverage (`scripts/coverage.sh`): `ultragraphdb_cluster_placement` 0 uncovered lines; `ultragraphdb_core_ids` (the
three edited files) 0. Uncovered, with their reasons:

- `ultragraphdb_error_cluster/src/errors/cluster_error/cluster_error_display.rs`, the last arm of `fmt_sequencer`: the
  placement and setting variants, which `fmt` writes itself and never passes on. The arm exists because the match is
  exhaustive without a wildcard; it writes the variant's `Debug` text rather than recursing or returning an error.
- `ultragraphdb_cluster_sequencer/src/types/sequencer/mod.rs`, the error returns of two `invariant(...)?` checks in
  `hold` (close times never fall; a transaction lies in the epoch after the last close): reached only through a bug,
  and in a debug build an invariant panics before it returns.

`cargo fmt -p` and `cargo clippy -p <crate> --all-targets -- -Wclippy::all` for the three crates and `core_ids` leave
them unchanged and report nothing; no `unimplemented` warning is left. Clippy's findings on the way were
`shadow_unrelated` in tests and the codec (renamed), `len_without_is_empty` (`BucketRange::len` became `buckets`, since a
range is never empty) and `manual_is_multiple_of` in a test. `buildifier --mode=check` passes on the three `BUILD.bazel`
files.

### 5. Fuzz

- `fuzz/ultragraphdb_cluster_placement`, target `placement_decode`: any bytes are decoded or refused as `Malformed` with
  an offset within the input; an accepted map re-encodes to its bytes, and its latest layout covers every bucket once,
  each range's first bucket resolving to its partition.
- `fuzz/ultragraphdb_cluster_sequencer`, target `record_decode`: any bytes are decoded or refused with one of the
  refusals a decoding raises (`Malformed`, `TooMany`, `NoPartitions`, `DuplicatePartition`, `EpochAhead`); an accepted
  record re-encodes to its bytes.
- Seeds: `testdata/fuzz_seeds/<target>/`, ten each, written by `testdata/seeds.py` (run with `python3 -I`) from the
  formats' documentation alone, valid ones and `malformed_*`; each crate's unit suite decodes every seed (a valid one
  re-encodes to its bytes, a malformed one is refused), which cross-checks each encoder against an independent one.
- `cargo +nightly fuzz build --fuzz-dir <crate>` (in `fuzz/`) built both with no warning of their own (the existing
  workspace warns about its unused `compio` pin). Each replayed its seeds once (`-runs=0`, 11 inputs each, no failure),
  from a copy of the seeds in the scratchpad so no corpus file landed in `testdata/`. The time-boxed runs of
  `scripts/fuzz.sh` belong to group 12.

## Decisions the specs left open (for the user's review)

1. **The initial map splits every tenant run, not only those of existing tenants.** The spec asks the tenants
   that exist split evenly and every other bucket assigned somewhere; `PlacementMap::even(partitions, leader)` splits
   all 256 runs alike, so a tenant created later is split evenly too and the ranges need no tenant list; Placement's runs list
   the tenants the metadata group holds, which may share a run. The
   alternative, other runs on one partition, would give a later tenant a single partition (cheap single-partition
   transactions, no spread).
2. **At most 1,024 partitions** (`PlacementMap::PARTITIONS_MAX`), hence at most `2^18` ranges per layout (every run
   split over every partition) and 1,024 subscribers and partitions per multi-partition transaction. With every run
   split, 65,536 partitions would mean `2^24` ranges.
3. **A change is a complete new layout** naming the epoch it applies from, strictly after the latest change's; the
   first layout applies from epoch 0. The map keeps at most 256 layouts (Ultra Style rule 2), refused as `Full` past
   that; nothing trims old layouts yet (stage 8's history horizon).
4. **Preferred leaders are a `NodeId` per partition**, a type the design's table does not list. `even` gives every
   partition the one node; a change drops the leader of a partition it empties and gives a new partition none until
   `set_preferred_leader`, which refuses a partition the latest layout does not hold.
5. **The sequencer owns its `OrderedLog`.** Deciding methods are synchronous; `recover` and `sync` await the log, as the
   commit step's `Pipeline` does. One record per round.
6. **Time is a `ClockReading` of two clocks the caller passes**: a monotonic one spaces closes, the wall clock is what a
   close records (clamped, never falling). One clock could not satisfy both "at most `epoch_ms_max` apart" and "the
   clock steps back by one second". After a restart the next close is due `epoch_ms_max` after the start, the old
   monotonic time being unknown.
7. **A close request names the epoch it wants closed**: the open epoch arms the early close, an epoch already closed
   (durable or not) is satisfied and changes nothing, a later one is refused (`EpochAhead`). Partition and frontend
   requests are the same call.
8. **Catalog versions and evictions are the caller's**: `tick` takes the pending versions (as `(namespace, version)`)
   and the evictions (as 128-bit session identities, `u128`, since no `SessionId` type exists yet; group 4 or 6 may add
   one to `core_ids`) and refuses more than 256 versions or 4,096 evictions per close with `TooMany`; the host passes
   at most that many. A close of the last epoch has no effective epoch (`Close::effective_epoch` is `None`).
9. **Bounded memory.** The sequencer keeps the last `closes_retained` closes (default 65,536) for catch-up and `as_of`,
   and at most `transactions_retained` transactions (default 65,536), refusing more as `Full`, since nothing executes
   or trims them before stage 8. A subscriber behind the oldest retained close is refused (`CatchUpTrimmed`); `as_of`
   before the oldest retained close names that close's time as "the first", which differs from the spec's "first
   close" once closes are trimmed.
10. **The sequencer's log is not trimmed.** Recovery replays from position 1; a log may begin at any close (accepted
    for a log whose earlier rounds a future checkpoint trimmed), after which every close is one after the last. The WAL
    pads each round to 4 KiB, so at 100 closes a second the default 4 GiB log fills in about three hours: a checkpoint
    policy for the sequencer (what a trimmed log keeps: the last close, the unexecuted transactions, the announcements
    the metadata group has not taken) is needed before the frontend runs for long, in group 9 or by the user's decision.
11. **Multi-partition transactions carry an opaque body** (at most 65,536 bytes) beside their partitions and reply epoch,
    for stage 8 to execute; partitions must be distinct, the first repeat named.
12. **The scenarios bound partly here.** "Epoch lengths out of their bounds stop the start" checks the refusal naming
    the setting and the bound; the exit code 78 is `RoleError`'s mapping, group 11. The four scenarios of "Partitions
    learn each closed epoch and stamp the open one" and the partition half of "A version reaches every partition before
    its epoch" are checked against a model of a partition the sequencer's deliveries drive; groups 4 and 7 run them
    against the epoch stamp and the partition. "A crash before the close leaves the version pending" models the
    metadata group's pending version as test state (group 6 holds it durably) and kills the frontend after the close of
    39 is appended and before it is durable.

## Deviations from the design's crate table

- `ultragraphdb_cluster_sequencer` depends on `ultragraphdb_cluster_placement` (for `PartitionId`, which subscriptions
  and transactions name), `ultragraphdb_error_invariant` (its invariants) and `ultragraphdb_error_log` (an exhausted
  position); the table lists `_core_ids`, `_error_cluster` and `ultragraphdb_traits_log`.
- Each crate holds a private big-endian `Reader` of about 60 lines; the two copies differ in what they read
  (`u128`, a byte run) and are not shared, since neither crate's API is the place for it.
- Phase 3 audited the implementation kept for phase 4, each defect planted and reverted in turn, as group 1 did.

## Files

- New: `ultragraphdb_error/ultragraphdb_error_cluster/`, `ultragraphdb_cluster/ultragraphdb_cluster_placement/`,
  `ultragraphdb_cluster/ultragraphdb_cluster_sequencer/` (each `Cargo.toml`, `BUILD.bazel`, `src/`, `tests/`; the two
  cluster crates also `features/` and `testdata/`), `fuzz/ultragraphdb_cluster_placement/`,
  `fuzz/ultragraphdb_cluster_sequencer/`.
- Changed: `Cargo.toml`, `Cargo.lock`, `AGENTS.md`, `fuzz/Cargo.toml`, `fuzz/Cargo.lock`,
  `ultragraphdb_core/ultragraphdb_core_ids/src/types/{namespace.rs,epoch.rs,bucket/mod.rs}` and its
  `tests/types/{namespace_tests.rs,epoch_tests.rs}`, `tasks.md`.
