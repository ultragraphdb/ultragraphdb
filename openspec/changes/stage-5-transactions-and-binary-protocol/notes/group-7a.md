# Group 7a: the commit step's rounds, versions and resolver

`ultragraphdb_transactions_commit`: tasks 7.3 and 7.4, and the parts of 7.1 and 7.2 that cover them. 7b builds 7.5 to
7.9 on this step function and finishes 7.1 and 7.2.

## 7.1 Scenarios (this half)

`features/commit-rounds/commit_rounds.feature` and `features/transactions/transactions.feature` hold, verbatim, run by
the crate's cucumber target `bdd` over real transactions (`ultragraphdb_transactions_txn::Transaction` on a changeset
applier over the simulated store), decided by the commit step, each synced round applied by a test driver standing in
for the pipeline (`tests/utils/fixture.rs::Driver`):

| Spec | Requirement | Scenario |
|---|---|---|
| commit-rounds | Commits are grouped into rounds | "A lone commit waits about one sync" |
| commit-rounds | Commits are grouped into rounds | "Commits arriving during a sync share the next round" |
| commit-rounds | Commit versions follow the clock | "A clock stepping back does not lower versions" |
| commit-rounds | Validation is exact | "A transaction is not aborted by another's aborted write" |
| commit-rounds | Validation is exact | "Two runs of one round decide alike" |
| commit-rounds | The conflict history is bounded by the oldest open read | "A transaction older than the history aborts" |
| transactions | Transactions are strictly serializable | "Write skew is refused" |

**Left for 7b**, with the requirement each belongs to:

- commit-rounds, "Acknowledgement at the visibility watermark": "No acknowledged commit is invisible" and "A failed sync
  acknowledges nothing". The step's side of the second is built and unit-tested
  (`commit_step_tests::test_a_failed_sync_acknowledges_nothing_and_stops_the_step`); the scenario belongs with the
  pipeline over a real `OrderedLog` (7.6).
- commit-rounds, "Catalog changes, drops and index builds are ordered in rounds": all five scenarios (7.5, 7.7).
- commit-rounds, "Rollups take no version from the commit step": "A rollup writes between commit versions" (bound by
  group 5 in `store_posting`; to bind again against the sequencer).
- transactions, "Commit-time operations" (six), "Exactly-once commits through sessions" (two), "Aborts name their
  reason" (two: busy and too large; the step refuses a request too large for any round, `CommitError::TooLarge`, but a
  write set past 1 MiB is refused first on the worker by `TxnError::WriteSetTooLarge`, so the scenario is the
  session's and the pipeline's to bind).

The four conflict scenarios of "What a transaction's reads record and what conflicts with them" are bound by group 4 in
`store_apply` against the resolver's rules; their conflict side is now also unit-tested against the real resolver
(`conflict_history_tests`, `commit_step_store_tests::test_inserts_of_one_new_statement_conflict_and_of_different_objects_do_not`).

## 7.2 Surface (this half)

| Item | What it is |
|---|---|
| `CommitStep<P: Proposal>` | The step function: `new(bounds, previous)` after the store's applied version (the watermark, the version commits follow and the history's floor); `step(input, now_us, &mut out)`; `watermark`, `in_flight`, `in_flight_admitted` (decided, unapplied), `waiting`, `is_stopped`, `resolver`, `bounds`, `previous_version` |
| `Input<P>` | `Commit(CommitRequest<P>)`, `Synced`, `SyncFailed`, `OldestRead(u64)` |
| `StepOutput<P>` | What one step asks, in the order the caller does it: `append` (the round closed, its `RoundRecord` and Round body; `round_ref(position, parent)` for `OrderedLog::append`), `applied` and `apply`/`apply_mut` (the synced round's admitted proposals at their versions, drained by the caller and given back), `replies`; `clear`, `capacity` |
| `CommitRequest<P>` | A ticket, routing the reply back, and the proposal |
| `Reply` | A ticket and `Result<u64, CommitError>` |
| `RoundRecord` | First version, transactions (every decision), committed; `last_version` |
| `RoundBounds` | Frame bytes (header and chunk table included, at most 1 MiB) and transactions (at least 1) |
| `Sequencer` | `open_round(now_us)`: `max(previous + 2, 2 × clock µs)`, even, the clock read once per round; `take` for each admitted transaction, `first + 2k`; `None` once exhausted |
| `Resolver` | `check(read_set)`: unsealed, too old, conflict, in that order, changing nothing; `admit(version, record)`; `prune(floor)`; `history` |
| `ConflictHistory` | The sorted runs (below): `insert`, `conflict`, `prune`, `floor`, `bytes`, `capacity`, `len`, `runs`, `RUN_CAP` |
| `Proposal` | `read_set()` and `record()`; implemented for `Prepared` |
| `CommitError(pub CommitErrorEnum)` | `Conflict { kind }`, `TooOld { read_version, floor }`, `TooLarge { size, limit }`, `Malformed { cause }`, `Unsealed`, `Unavailable`; `code`, `detail` (the wire's `ErrorCode` and detail, as the Round decision records them), `is_retryable` (conflict and too old) |

Dependencies added: `ultragraphdb_error_protocol` (the cause of `Malformed`). Dev-dependencies: the simulated store and
applier stack for the end-to-end tests, `ultragraphdb_transactions_log` (`WalConfig`, whose round bounds the node passes
in), cucumber, futures, criterion. The skeleton's `ultragraphdb_error_log` and `ultragraphdb_store_apply` stay declared,
unused by 7a; 7b's pipeline and recovery use them.

## Performance by design

| Choice | Why | Tested by |
|---|---|---|
| The history as sorted runs, each transaction's writes one run, merged into the run before it while that one is no larger (a binary counter), capped at 16,384 entries a run | a logarithmic number of small runs at the young end, so a check at a recent read version walks few runs; a merge copies a bounded amount; no tree node per key | `test_runs_merge_to_a_logarithmic_number`, `test_a_merge_stops_at_the_run_cap`; merging never (H4) and the cap ignored (H5) planted |
| A run's keys in one byte arena named by u32 spans; points `(key, whole, touched)` and statements `(key, object, version)` as plain vectors | one allocation per buffer, not per key; no `Rc`/`Arc`/`Box`, no hash map | `test_the_bytes_count_the_keys_and_24_per_entry` |
| Emptied runs kept in a pool of 8 and reused | no allocation per transaction in the steady state | `test_a_steady_state_allocates_nothing_more`, `test_pruning_keeps_only_a_bounded_number_of_runs_for_reuse` (H9) |
| A statement also records its list as touched, so a list read, an absent read and a range read are one lookup among the points | no scan over a list's objects; exact, since a touched key conflicts only with what reads the whole list | `test_a_list_read_whole_conflicts_with_…`, the model test; F2 planted |
| A check walks the sealed read set beside each run holding a version above the read version, each step binary-searching the rest of the run (a merge join) | O(R log W) per run with no per-check allocation; runs at or below the read version skipped | the model test; C10, C12 planted |
| Pruning drops whole runs from the old end, each in constant time; a merge leaves out entries at or below the floor | pruning bounded in cost per round; dead entries go at the next merge | `test_pruning_raises_the_floor_…`, `test_a_merge_leaves_out_…` (H8) |
| Memory accounted: `bytes()` (arenas and 24 bytes per entry) and `capacity()` | the history's footprint is observable | `test_the_bytes_count_the_keys_and_24_per_entry` |
| The Round body written straight into the output's reused buffer in `RoundBody`'s layout, reserved once, each record copied once; `RoundBody` (a `Vec<Vec<u8>>`) not used | one copy per record, no allocation per transaction | `test_a_round_s_body_is_the_wire_s_round_body` (byte-equal to `RoundBody::body`); P8, P10 planted |
| The step's tables (`waiting`, `decided`, `admitted`) reused; the applied round's vector swapped with the caller's | no allocation per transaction or round once warm | `test_a_steady_state_reuses_the_output_s_buffers` |
| No error built on the success path; validation exact, every key compared byte for byte | — | review; the model test |

**Differential tests.** `conflict_history_model_tests` runs the history against `tests/utils/model.rs`, which keeps every
commit's writes whole and checks every read against every write above its read version pair by pair: 40 seeds × 400
operations (commits at rising versions, prunes at random floors, checks at random read versions at and above the floor,
odd ones included), keys from a three-letter alphabet so they repeat, share prefixes and fall in each other's ranges;
over 5,000 checks with between 10% and 90% conflicting, asserted. A second run puts the resolver against the model with
read versions straddling the floor: too old just below it (over 100 cases), the model's conflict at and above it.

**The bench** (`benches/history_bench.rs`; `cargo bench -p ultragraphdb_transactions_commit --bench history_bench`,
`bazel run //ultragraphdb_transactions/ultragraphdb_transactions_commit:history_bench -- --bench`; never in
`make test`). Runs against `BTreeMap<Vec<u8>, u64>` of points and `BTreeMap<(Vec<u8>, Vec<u8>), u64>` of statements,
pruned through a log of each transaction's keys (design.md's sketch). Each transaction writes 8 forward-list keys of 25
bytes and 4 statements and reads 16 lists and 4 statements, subjects from a million. Measured 2026-10-28 on the development Mac (Apple silicon), 1 s
warm-up, 3 s measurement, medians, over a history holding the last `window` transactions:

| Per transaction | Window | Runs | `BTreeMap`s |
|---|---|---|---|
| Admit (insert and prune, steady state) | 1,000 | 3.14 µs | 5.05 µs |
| Admit | 10,000 | 3.03 µs | 4.07 µs |
| Check, recent read (4 transactions above it) | 1,000 | 1.71 µs | 4.54 µs |
| Check, recent read | 10,000 | 2.50 µs | 7.63 µs |
| Check, old read (the whole window above it) | 1,000 | 24.0 µs | 4.15 µs |
| Check, old read | 10,000 | 34.4 µs | 2.27 µs |

The runs admit 26% to 38% faster and check a recent read 2.7 to 3 times faster, allocating nothing once warm; the maps
allocate two owned keys per write (the map's and the log's). A read whose version is far below the newest checks
every run above it, about 20 at a window of 10,000, and is 6 to 15 times slower than the maps, which pay one lookup
per key whatever the read version (and stop at the first hit, which an old read finds sooner). Such a read is a
transaction that lived long, bounded by the 10 s limit, so the common case is the recent one; the old-read cost is the
price, recorded as an open question below.

## Corner cases, enumerated before the suite

| Case | Test |
|---|---|
| Versions: from 0, at a slow and a fast clock, a clock stepping back, the clock read once per round, an unused round, an odd previous version, no round open, exhaustion before and within a round and by the clock's overflow | `sequencer_tests` (all), `test_exhausted_versions_stop_the_step` |
| A lone commit on an idle step; commits during a sync; a sync with nothing in flight | `test_a_lone_commit_…`, `test_commits_during_a_sync_wait_…`, `test_a_sync_with_no_round_in_flight_does_nothing` |
| The count bound at 3 with 7 waiting (3, 3, 1); a bound of 0 taken as 1; bytes exactly two records' frame and one byte less; a request one byte over a round of its own | `test_a_round_holds_at_most_…`, `round_bounds_tests`, `test_a_round_holds_what_fits_its_bytes`, `test_a_request_too_large_…` |
| The bounds of both power-loss-protection defaults (256 and 1,024) | `test_the_bounds_follow_the_power_loss_protection_flag` |
| A round of commit, abort, commit (the abort takes no version, the body equal to the wire's); a round of aborts alone (the watermark stays) | `test_a_round_s_body_is_the_wire_s_round_body`, `test_a_round_of_aborts_alone_keeps_the_watermark` |
| Each read kind against each write kind, inside, outside, at both range ends, open ends, an empty range, another namespace's catalog | `conflict_history_tests` (the first seven) |
| A write at the read version, above it by one (odd read version) and by two; the newest of two writes of one key | `test_only_writes_above_the_read_version_conflict`, `test_the_newest_write_of_a_key_decides` |
| Floor: at, below and above a read version; never lowered; above the watermark (clamped); after a restart | `test_pruning_raises_the_floor_…`, `resolver_tests`, `test_the_oldest_read_prunes_…`, `test_a_step_opened_after_a_version_…` |
| Duplicates in one transaction and across merged runs; a key written whole and by a statement in one transaction; a transaction writing nothing | `test_a_key_written_twice_is_held_once`, `test_a_list_written_whole_and_by_statements_…`, `test_a_transaction_that_wrote_nothing_adds_no_run` |
| A failed sync with a round in flight and requests waiting; requests after it; a sync after it | `test_a_failed_sync_acknowledges_nothing_and_stops_the_step` |
| A malformed record; an unsealed read set; both at once | `test_a_malformed_record_and_an_unsealed_read_set_are_refused`, `test_a_malformed_record_is_refused_before_the_read_set_is_checked` |
| Every `CommitError` variant: constructor, code, detail, retryability, text, source | `commit_error_tests` |

Independent sources: versions worked by hand from the formula (written in each test's comment); frame sizes and Round
bodies from `ultragraphdb_protocol_wire`'s own `RoundBody` builder; the bounds' defaults from `WalConfig`; conflicts from
the reference model; end-to-end outcomes read back from the store by a fresh transaction.

**Input variety (coincidence review).** Versions and read versions are distinct in every test that asserts one (a
write at 12 read at 10, a floor of 20 with read versions 19 and 20, clock rounds at 20,000,000 and 20,000,002); tickets
differ from versions; the bounds' bytes and count are never both binding in one test; the model's keys repeat by
design, and the end-to-end scenarios use distinct subjects per transaction so that only the intended read conflicts.

## Phase 2: the failing run

`cargo test -p ultragraphdb_transactions_commit` against the phase-1 surface, 2026-10-28 12:32 (recorded then; this
section was rewritten into place when the notes were assembled):

- unit (`tests/mod.rs`): 69 tests, 64 failed, every one at an `unimplemented!()` of the surface (`RoundBounds::new` 23,
  `ConflictHistory::new` 18, `Resolver::new` 9, `Sequencer::new` 8, and one each at `StepOutput::new`,
  `RoundRecord::last_version`, `Prepared`'s `Proposal::read_set`, `CommitError::code`, `is_retryable` and `Display`).
  The 5 that passed exercise only what phase 1 wrote whole: the constructors and getters of `CommitRequest`, `Reply`,
  `RoundRecord` and `CommitError`, and `CommitError::source`.
- bdd (`tests/bdd.rs`): 7 scenarios, 7 failed, each step at `not implemented`.
- No compile error.

The audit later added 8 unit tests (77 in all).

## Phase 3: the audit

74 defects, each planted alone in the tree by a script in the session's scratchpad and restored in a `finally`, the
crate's unit and BDD targets run under Cargo; `src/` hashed before the first plant and checked after the last (equal).
The first run was interrupted after two plants to move the build into a target directory of its own (the IDE held
Cargo's lock); the interrupt restored the tree, checked by hash.

| Result | Defects |
|---|---|
| Rejected at once by a test aimed at the rule | 64 |
| Survived; a test added, the defect planted again and rejected | 8 (below) |
| Survived, equivalent | 2 (below) |
| Broken plants | none |

The 64: S1–S5 (versions: odd after an even previous, min for max, the clock not doubled, a step of 1, the previous not
kept), B1, B2 (bounds), R1, R2 (the last version), E1–E4 (codes and details), F2–F5 (a statement's list not touched,
statements unsorted, a run's newest lost, points not deduplicated), M1–M4, M6 (merges: a key's older whole version, the
floor overshot, one side's statements and ranges dropped, a statement's older version), C1–C10, C12 (checks: touched
ignored, `>=` for `>`, a statement's list written whole ignored, objects not compared, range ends inclusive, one key a
range, absent keys and catalog overlaps of drops, overlap and containment off by one, the seek past an equal key, the
statement cursor reset), H1–H7, H11 (runs at the read version skipped, kinds misreported, the catalog or absent check
gone, merging never or past the cap, pruning at or lowering the floor), V1–V4 (too old at the floor, unsealed accepted,
an empty range end as a key, a point lost), P1–P8, P10–P15, P17, P19, P20 (rounds: the count off by one, the bytes `>=`,
the record table left out of the frame, the too-large margin, no round at once, the watermark kept, an abort adding
writes, record numbers from 1, a section offset, the floor not clamped, the restart floor 0, waiting requests left
unanswered or the step left running, exhausted versions ignored, transactions counted as commits, aborts replied as
commits, a failed sync taken as synced), X1 (`Prepared`'s record empty), X2 (an abort's detail dropped).

| Survivor | Why it survived | Closed by |
|---|---|---|
| S6, an exhausted round keeping the round before's version | no test exhausted after an open round | `test_an_exhausted_round_forgets_the_version_of_the_round_before` |
| F1, a key written whole and by a statement keeping only the touched version | the tests' small runs kept the whole entry first | `test_a_list_written_whole_and_by_statements_in_one_transaction_is_held_as_both` (40 keys, so the sort reorders) |
| C11, a drop over the catalog range missed | no drop covered a catalog | `test_a_range_written_over_the_catalog_conflicts_as_a_drop` |
| H8, a merge never dropping what is below the floor | memory only; no test counted entries after a merge across the floor | `test_a_merge_leaves_out_what_is_at_or_below_the_floor` |
| H9, the pool unbounded | memory only | `test_pruning_keeps_only_a_bounded_number_of_runs_for_reuse` |
| H10, an empty run pushed | no test counted runs after an empty write set | `test_a_transaction_that_wrote_nothing_adds_no_run` |
| P16, a round closed by a sync reading clock 0 | every such round had a slow clock | `test_a_round_closed_by_a_sync_reads_the_clock_then` |
| P18, the record parsed after the resolver's checks | no proposal failed both | `test_a_malformed_record_is_refused_before_the_read_set_is_checked` |

| Equivalent | Argument |
|---|---|
| M5, a merge's newest version not taking touched versions | a touched version is always also the version of a statement of the same key in the same run (`fill` writes both; a merge keeps or drops both by the same floor and takes the maximum of both), so the statements' loop already raises `newest` to it |
| P9, no padding between records in the Round body | every write-set record is a multiple of 8 bytes long: its sections start aligned and every key-value entry is padded to 8, so its end, the end of its last section, is aligned (a record that is not fails `WriteSetRecordView::new` and is refused before it is admitted). The padding is kept because the layout states it |

The phase-3 implementation is kept, as the tasks' header allows.

## Final counts

Equal under both build systems, 2026-10-28: unit 77 tests (`cargo test -p ultragraphdb_transactions_commit --test unit`,
`bazel test //ultragraphdb_transactions/ultragraphdb_transactions_commit:ultragraphdb_transactions_commit_tests`), BDD
7 scenarios and 14 steps (`--test bdd`, `:bdd`), the doc test target empty. `cargo build --workspace`, `make format` and
`make fix` clean; the bench builds under Bazel (`:history_bench`). `make test` is the orchestrator's.

## Phase 5

The crate decodes no bytes from a client, a peer or the disk: a record is read only through
`ultragraphdb_protocol_wire::WriteSetRecordView`, which `protocol_wire`'s targets fuzz. No fuzz target. Mutation testing
runs at the stage's close (11.8).

## Open questions for the user

- **An old read's cost.** A transaction reading far below the newest version pays a check per run above it (34 µs at a
  window of 10,000 transactions, against 2.5 µs for a recent one). Is that acceptable until group 12's measurements
  (12.2, 12.3) show the share of such transactions? If not, the recourse that keeps validation exact is a per-run key
  range or filter consulted only to skip a run that cannot hold a key, never to decide a conflict.

## Decisions taken here, for review

- **The history is sorted runs, not a `BTreeMap`.** design.md sketches "a `BTreeMap` of written keys and statements to
  versions and an interval map of written ranges"; the performance rules of tasks.md ("keys held once and named by u32
  spans", "sorted read sets merge-joined against the history", no allocation per transaction) and the bench below
  settle it as runs. Ranges written are held per run in a plain vector, checked linearly: only drops write them (7b),
  which are rare.
- **One history for both checks.** A transaction is checked once, against the history, which holds the round's admitted
  transactions at their versions. This is spec's two checks in one: a read version is at most the watermark, and every
  version of the current round lies above every version decided before it, so "above its read version" includes every
  write of its round admitted before it.
- **What conflicts, beyond the spec's table.** A statement written counts as its list touched for a list read, an
  absent read and a range read; a statement read conflicts with its list written whole. An absent key conflicts as a
  list read of that key does. A written range conflicts with every read whose key or range it shares, reported as
  `Drop`. The kinds are reported in the order list, statement, range, absent term, catalog, then drop.
- **A range's end in the record.** Section 8 holds a range's end key as its value; an empty end reaches past the last
  key (an empty key is below every key, so `[start, "")` would be empty, and no range is written empty).
- **The floor.** The oldest open read-write transaction's read version arrives as `Input::OldestRead`, computed by the
  caller (the workers know their transactions; the commit step does not until they commit), clamped to the watermark,
  never lowered. A commit step opened after version v starts with the floor at v, since the writes before it are not
  in its history: a transaction reading below v is too old.
- **The round's clock and versions.** The clock is read once per round, when it closes; transaction k committed in it
  takes `first + 2k`, as `RoundBody` states; an aborted transaction takes none; a round of aborts alone records the
  version its first commit would have taken and leaves the watermark.
- **Bytes bound.** A round's bytes are its frame's: the 256-byte header, the body and its chunk table, every request
  counted as committed. A request whose round alone exceeds the bound is refused as too large when it arrives, so it
  never blocks the queue.
- **Replies go out as a unit** once the round's sync completes, aborts included. A failed sync answers the round, the
  queue and every later request `Unavailable` (non-retryable, the wire's "refuses writes after a failed log sync").
- **Versions exhausted** past `u64::MAX` stop the step as a failed sync does, rather than repeating or wrapping.
- **`CommitError` stays in the crate.** Only the commit step raises it; the session (group 10) reads it through this
  crate, which it depends on anyway, so no `ultragraphdb_error/` crate.
- **Bounds from the log's configuration.** `RoundBounds::new(bytes, transactions)` takes `WalConfig::round_bytes` and
  `round_transactions`, where the power-loss-protection defaults already live, rather than repeating the table; a test
  pins both defaults through the step.
- **Tests' doubles under `tests/utils/`**, as `ultragraphdb_transactions_txn` keeps its fixture, and shared with the BDD
  target by path; no `src/utils_test/`.

## What 7b should expect

- **Deciding one transaction** is `CommitStep::decide` (`src/types/commit_step/commit_step_round.rs`): the record parsed,
  `Resolver::check`, then `Sequencer::take`, then `Resolver::admit`. The Lean model's order puts the session check
  before validation and commit-time operations after it and before the version: sessions go first in `decide`, and the
  operations between `check` and `take`, a refusal (`OperationFailed`) returning before a version is taken, so it adds
  no writes. The operations' writes must reach the history too: admit the record's conflict sections and the
  operations' own (or extend the record before `admit`).
- **Decided but unapplied** writes for the operations are `CommitStep::in_flight_admitted` (the round syncing) and the
  current round's `admitted` so far.
- **The pipeline** drives `StepOutput` in its order: append and sync the round (`round_ref` at the log's tail), apply
  `apply_mut().drain(..)` with `apply_round` (`Prepared::into_parts`), then send the replies; feed `Synced` or
  `SyncFailed`. Aborted proposals are dropped by the step today; a worker wanting its `TxnBuffers` back from an aborted
  transaction needs them handed out (an output table of released proposals).
- **Catalog changes, drops and builds** reach the resolver as conflict points (catalog keys, inside every read set's
  catalog range) and conflict ranges (drops, `ConflictKind::Drop`). The implicit default catalog entry must stop being a
  catalog write in the record (7.5), or concurrent first writes of a predicate conflict on `Catalog`, as the scenarios'
  set-ups here avoid by writing each predicate first.
- **Recovery** opens the step with `CommitStep::new(bounds, applied_version)`; the history starts empty at that floor.
- **Sessions**: `Reply` carries the ticket only; the session and request numbers are in the record's fixed part.
