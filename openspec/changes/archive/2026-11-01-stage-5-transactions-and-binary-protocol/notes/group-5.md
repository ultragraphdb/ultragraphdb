# Group 5: execution leaves the commit lock

## 5.1 Scenarios

- `ultragraphdb_store_posting/features/local-commit/local_commit.feature` holds the ten scenarios of local-commit's three
  modified requirements, verbatim, replacing the stage-2 text: "A transaction in the same round does not read
  another's writes" replaces "A later request in a batch reads an earlier one", and the others take the new wording
  (transactions, rounds, commits at 12 and 14, a read at 10 while the commit at 10 is applied, a rollup at 9 above
  deltas at 6 and 8).
- `ultragraphdb_store_posting/features/commit-rounds/rollups.feature` holds commit-rounds' "A rollup writes between
  commit versions": the rule is the posting crate's (5.2, 5.4), while the requirement belongs to the commit step,
  which binds it again in group 7.
- `ultragraphdb_store_apply/features/changesets/changesets.feature` gained "A provisional UID is confirmed at commit";
  "A delete by pattern removes every object" was there already, unchanged.
- Rebound to the new mechanism: schema-storage's "Concurrent first writes both commit" executes both changesets from
  one watermark and writes them as one round; posting-cache's "a list is cached, a commit at 20 changes it, and it is
  read at 21" commits another list at 22 first, since a read at 21 waits until the watermark reaches it.

## 5.2 Phases 1 to 3

### Surface

- `ultragraphdb_store_posting`: `Writes` (what a request writes apart from it: deltas, entries at the commit version and
  at their own, versionless writes, the epoch mark, removed rebuild records; getters only); `Request::into_writes`,
  which closes the request's read; `Lists::commit_round(&[(u64, &Writes)])`; `Lists::next_version`;
  `PostingError::StaleVersion { version, after }`.
- Removed: `Lists::fresh_ts`, `latest_ts` and `peek_ts`; `Timestamps::new`, `fresh`, `peek` and `observe`
  (`Timestamps` keeps the open reads and the discard bound, and is `Default`). `Lists::applied_ts` is the visibility
  watermark.
- `ultragraphdb_store_apply`: `Executed` (namespace, read version, `Writes`, `WriteTerms`, `Reads`; the catalog, build,
  touched nodes and index removals are the crate's), `ChangesetApplier::execute(&Changeset) -> Executed` and
  `ChangesetApplier::apply_round(Vec<(u64, Executed)>) -> Vec<Applied>`. Removed: `ChangesetApplier::apply_batch`.
  `apply` stays: execute, then a round of one at `next_version`, under the lock rounds take.

### Decisions taken here, for review

- **Who gives versions before the commit step exists.** `apply_round` takes the versions it is given and checks only that
  they rise above the watermark (`StaleVersion`). Writes the commit step does not order yet take
  `Lists::next_version`, the smallest even version above the watermark: `ChangesetApplier::apply` (execute, then a
  round of one, under the lock rounds take, so nothing commits between the two), a batch (`Lists::commit`, two apart
  per request kept) and a drop. Every commit version is even, so a rollup, one above the newest commit it folds, never
  takes one. `Timestamps` gives none. Group 7's sequencer replaces `next_version` for transactions; 7.7 orders drops and
  the rebuild's swap in rounds.
- **`apply_batch` is removed**, its callers moved to `execute` and `apply_round` (tests) or to `apply` per changeset
  (the bench and the rollup bench). A batch's changesets read each other's writes through stacked layers, which a
  transaction no longer does; executing several from one watermark and writing them as one round without a resolver
  could admit two that conflict. The bench's apply step is now one round per changeset, so its figures are not
  comparable with stage 4's; group 12 measures the new pipeline.
- **`Batch` and `StackedLayers` stay**, used by the rebuild's swap, undo and record removal (single requests) and by the
  tests of `store_posting`, `store_quads` and `store_hnsw`, as the design says; no transaction uses them.
- **`apply_round` validates nothing a resolver decides.** It refuses only what would corrupt the store or break a rule of
  this crate: a stale version, and a second index build (one runs at a time), the whole round, before anything is
  written. The new-IRI check `QuadStore::commit` made is not in the round path: it is one rule of the resolver (7.4).
  Two executions creating one IRI both record its dictionary key as absent (group 4), which is what the resolver checks.
  An execution's dictionary entries are added to its `Writes` at execution.
- **The read gate.** A read at or below the watermark never waits. A read above it waits until the watermark reaches it,
  as the spec says; once a write has failed it is refused with `Poisoned` rather than left waiting for a watermark that
  will never move. A read at `u64::MAX` reads everything applied and waits only for a write under way, which keeps the
  stores' own "read everything" reads (`List::read` through `snapshot(u64::MAX)`, the tests' read-backs) working.
- **A drop moves the watermark to its version** once done. The store does not record a drop's version as applied, so a
  reopened store's watermark may be below it; drops are physical, so later commits below it find nothing to land under.
- **Rollups read at the watermark and write at the newest commit they fold plus one, above the watermark until the next
  commit.** A read at the watermark still reads the deltas, and a second rollup before the next commit writes the same
  list again at the same version (D5 allows it). Tests that read a rollup's result read at `u64::MAX`. The early
  refusal of a rollup after a failed write went: `rollup_key` refuses under the writer lock anyway (an equivalent the
  audit found, below).
- **Index builds in a round.** A build starts at its own execution's version, not the round's first; the nodes a write
  changes during a build are recorded at `apply_round`, in round order, which is D7's "by the commit step". A rebuild
  record's number is the round's first version, which no other round takes, in place of a counter's fresh timestamp.
- **The catalog an execution reads** is the catalog the last applied round left, at or after the execution's watermark.
  A catalog change between execution and commit is the catalog-range conflict of D7, the resolver's.
- **`Writes` holds copies** of the request's deltas (one clone per list written); the old commit borrowed them.
- **Ported Go tests that read at timestamps above the newest commit** now commit another list first to take the
  watermark there, or build their lists after writing the store directly, so the lists start at the store's applied
  version; the posting and hnsw workloads read up to the watermark instead of past it. The posting workload commits
  rounds (independent requests at even versions with gaps) as well as batches, with the rule "a transaction reads its own
  writes on the state at its read version, and no other's".

### Corner cases, enumerated before the suite

| Case | Test |
|---|---|
| A round of one, of two at versions with a gap, empty | `lists_commit_tests::test_a_round_writes_each_transaction_at_its_version_in_one_buffered_batch`, `test_an_empty_round_writes_nothing`, `changeset_core_tests::test_an_empty_round_writes_nothing` |
| A version equal to the watermark, below it, equal to the previous, below the previous; an odd version above | `test_a_round_with_a_stale_version_is_refused_whole` |
| The next version from 0, from an even and an odd watermark, near and at `u64::MAX` | `lists_tests::test_the_next_version_is_the_even_one_above_the_watermark` |
| Every kind of write in a round: entries at the version and at their own, versionless, records, the epoch | `test_a_round_writes_every_kind_of_write`, `writes_tests` |
| A delta written and cancelled out; a list read but not written | `writes_tests::test_a_requests_writes_are_everything_it_writes` |
| Reads at the watermark, between it and a round's version, at the version, at `u64::MAX`, during and after the write | `test_a_read_waits_for_a_round_below_its_timestamp`, `test_a_read_of_everything_waits_only_for_a_round_being_written`, the scenarios |
| A failed round: reads at the watermark, above it, at its version; a later round | `test_a_failed_round_stops_writes_and_the_reads_above_it`, `test_a_failed_commit_stops_writes_and_the_reads_above_it` |
| A rollup of a list whose newest commit is below the watermark, at it; a rollup again before and after the next commit | `test_a_rollup_reads_at_the_watermark_and_takes_no_version`, `test_a_list_without_deltas_is_not_rolled_up`, "A rollup writes between commit versions" |
| A drop at the next version; one given its timestamp; one that fails | `lists_drop_tests` |
| Two executions from one watermark writing the same statement and creating the same IRI | `changeset_core_tests::test_executions_from_one_watermark_do_not_see_each_other` |
| Executions on threads of their own | `test_executions_run_on_other_threads` |
| A round with a build second; two builds; a build while one runs, at execution and at the round; a stale round with a declaration | `test_a_build_starts_at_its_own_executions_version`, `test_a_round_starting_a_second_build_is_refused_whole`, `test_a_stale_round_writes_no_rebuild_record` |
| An execution never applied: its catalog change and its terms | `test_a_round_installs_the_catalogs_its_executions_leave` |

Independent sources: versions and counts worked by hand from the round's versions (8 and 12, 10 and 12 folded at 13,
deltas at 6 and 8 at 9); the store's contents read through the simulator's own `contents()`; the catalog compared
with the catalog loaded from the store.

### Phase 2: the failing run

`cargo test --no-fail-fast` against the surface, the tests moved off `fresh_ts`, `latest_ts` and `peek_ts`:

- `ultragraphdb_store_posting` unit: 259 tests, 32 failed, each at an `unimplemented!()` (`commit_round`,
  `next_version`, `into_writes`, `StaleVersion`); BDD: 33 scenarios, 16 failed, 14 at `unimplemented!()` and 2 at a
  wrong expectation the new rule exposed: a rollup's result read at the watermark, which a rollup now lies above
  ("A corrupted value is refused", "Facets survive a rollup"); both now read at `u64::MAX`.
- `ultragraphdb_store_apply` unit: 119 tests, 12 failed: 10 at `execute` and `apply_round`, and 2 drop tests
  asserting that a drop's timestamp is the watermark, which the old counter did not give; BDD: 77 scenarios, 3 failed:
  2 at `execute`, and "Dropping a namespace's data leaves no split part", the rollup read again (now at `u64::MAX`).
- `ultragraphdb_store_hnsw` unit: 92 tests, 3 failed (the workload, at `next_version`).
- No compile error.

### Phase 3: the audit

The implementation was written, then audited with defects applied one at a time against the owning crate's unit and
BDD targets (scripted, as in group 4), and kept. Before the defects, a probe that panicked wherever a read waited
above the watermark found every test of the four crates that read past the newest commit (seven unit tests, one
scenario); each was rewritten as the decisions say.

| Defect | Caught by |
|---|---|
| E01 a round at the watermark taken | `test_a_round_with_a_stale_version_is_refused_whole`, `test_the_next_version_is_the_even_one_above_the_watermark` |
| E02 only a round's first version checked | `test_a_round_with_a_stale_version_is_refused_whole` |
| E03 a round written in the configured persist mode | `test_a_round_writes_each_transaction_at_its_version_in_one_buffered_batch` |
| E04 the watermark set to the round's first version | the suite hangs: reads at the round's later versions wait for ever (Bazel's test timeout fails it) |
| E05 the store's applied version the round's first | 4 tests, the posting workload |
| E06 the watermark moved before the batch is written | 3 tests; "A commit is visible whole", "A read waits for a commit below its timestamp" |
| E07 the next version odd | 11 tests; "A rollup writes between commit versions", the split-rollup crash scenario |
| E08 the next version wrapping at `u64::MAX` | `test_the_next_version_is_the_even_one_above_the_watermark` |
| E09 a batch's versions one apart | 3 tests, the faulty posting workload |
| E10 a read above the watermark goes ahead | 5 tests; two scenarios |
| E11 a read of everything waits for nothing | `test_a_read_of_everything_waits_only_for_a_round_being_written` |
| E12 a read above the watermark after a failure waits | `test_a_failed_commit_stops_writes_and_the_reads_above_it`, `test_a_failed_drop_advances_the_epoch_and_stops_writes` |
| E13 a drop leaves the watermark | 3 drop tests |
| E14 a drop at an odd version | 3 drop tests |
| E15 a rollup reads everything | 4 tests; "A rollup leaves an open read intact" |
| E16 a rollup ignores a failed write | equivalent: `rollup_key` refuses under the writer lock; the early check was removed |
| E17 a request's writes keep emptied deltas | `writes_tests::test_a_requests_writes_are_everything_it_writes` |
| E18 the versioned entries dropped | 4 tests |
| E19 the entry keys leave out the versioned | `test_a_requests_writes_are_everything_it_writes` |
| E20 a round removes no rebuild record | 2 tests |
| E21 a round never advances the epoch | 4 tests |
| E22 every request advances the epoch | 9 tests; two cache scenarios |
| E23 an execution writes no dictionary entries | 89 tests |
| E24 a second build allowed in a round | `test_a_round_starting_a_second_build_is_refused_whole`, `test_two_builds_executed_together_refuse_the_second` |
| E25 a round installs no catalog | 43 tests |
| E26 a round records no touched node | the two vector-build tests |
| E27 every commit of a round given its first version | `test_a_round_writes_its_executions_at_their_versions_in_one_batch`, `test_executions_run_on_other_threads`; "Concurrent first writes both commit" |
| E28 an execution ignores a running build | 7 tests; "Writes during a rebuild are indexed", "A failed rebuild keeps the old index usable" |
| E29 `apply` commits at the watermark | 100 tests |
| E30 a build starts at the round's first version | gap, closed by `test_a_build_starts_at_its_own_executions_version` |
| E31 stale versions checked only by the posting lists, after the rebuild record | gap, closed by `test_a_stale_round_writes_no_rebuild_record` |
| E32 an execution reads one below the watermark | 30 tests |

E30 and E31 survived the first run and were closed by the tests named; both were rejected on the rerun. E04 is
rejected by a hang rather than a failure: the test that asserts the watermark (`test_a_round_writes_each_transaction_
at_its_version_in_one_buffered_batch`) runs in the same binary as reads that then wait.

## 5.3 and 5.4 Phase 4

- 5.3: `ChangesetCore::apply_batch` split into `execute` (no lock the commits take: the request at the watermark,
  nothing stacked, the catalog as the last round left it, terms resolved through the dictionary with provisional UIDs
  from the leases) and `apply_round`/`write_round` (under the apply lock: the round's checks, its rebuild record, the
  touched nodes of a running build, `Lists::commit_round`, then the catalogs, the index removals and the build start
  in round order). Transactions no longer use `Batch` or `StackedLayers`; a round's store batch is written with
  `PersistMode::Buffer` whatever the lists' configuration, which the batch path keeps; `Timestamps` keeps the open reads
  and the discard bound and gives no version.
- 5.4: `Lists::rollup_key` reads at the watermark and writes at `List::rollup_entries`' version for a finite read, one
  above the newest commit it folds; it takes no version and waits for nothing.
- Outside the two crates, for the build: `store_hnsw`'s and `store_quads`' tests, `tool_test`'s changeset workload and
  `tool_bench` (`latest_ts` to `applied_ts`, `apply_batch` to `apply`, the rollup bench's discard at the watermark plus
  one, above every rollup its run wrote).

Counts, equal under both build systems (`cargo test -p` and `bazel test` of each package):

| Crate | Cargo | Bazel |
|---|---|---|
| `store_posting` | unit 259, BDD 33, doc 1 | the same |
| `store_quads` | unit 24, BDD 20, doc 0 | the same |
| `store_apply` | unit 121, the three workload targets 1 each, BDD 77, doc 1 | the same |
| `store_hnsw` | unit 92, BDD 7, doc 1 | the same |
| `tool_bench` | unit 61 (and 3 in its library), doc 0 | the same |

`bazel test` of `store_posting`, `store_quads`, `store_apply`, `store_hnsw`, `store_kv`, `tool_test` and `tool_bench`:
28 targets, 28 passed. The changeset workload (`workload_seeds`, `workload_faults`, `workload_replay`) runs every
changeset through `execute` and `apply_round` under every fault class, and the posting workload runs rounds beside
batches under the posting faults.

### Open questions

- **"Concurrent first writes both commit" (schema-storage) against D7.** Two changesets naming `score` for the first
  time both write its default catalog entry, and the scenario says both commit. D7 makes a catalog change a write to
  the namespace's catalog range, which aborts every concurrent read-write transaction of the namespace. Group 7 must
  decide whether an implicit default entry is a catalog change; if it is, the scenario cannot hold. Here it is bound
  as one round of two executions, with no resolver.
- **A round holding a declaration that starts a build, and writes of its namespace after it.** Those writes executed
  without the build running, so they maintain only the old indexes, and the build, reading at the declaration's
  version, does not see them. `apply_round` relies on the resolver's catalog-range rule (D7) to abort them; until group
  7, `apply` writes one changeset per round, so it cannot happen.
- **A read above the watermark waits without bound**, as the spec says. A client naming a future read version could
  hold a worker; groups 6 and 10 should refuse a read version above the watermark or bound the wait.

## 5.5 Phase 5: deferred to 11.8

Mutation testing runs once, at the stage's close (11.8, decided 2026-10-27); nothing in this group decodes bytes from a
client, a peer or the disk, so it has no fuzz target.
