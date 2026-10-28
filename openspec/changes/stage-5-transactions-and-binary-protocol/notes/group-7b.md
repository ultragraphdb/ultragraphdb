# Group 7b: the commit step's commit-time operations, pipeline, sessions and recovery

`ultragraphdb_transactions_commit`, with changes in `ultragraphdb_store_apply` and `ultragraphdb_transactions_txn`.

## Phase 2: the failing runs

Each item's suite was written against its surface and run before its implementation.

- **Implicit default entries (item 4, 2026-10-28 13:21).** 4 tests failed, no compile error: `store_apply`
  `test_an_execution_names_the_default_entries_it_writes` (at `Executed::defaults`' `unimplemented!()`) and
  `test_a_round_keeps_the_default_entries_of_the_executions_before_it` (the second execution's catalog replaced the
  first's: `age` lost); `txn` `test_a_predicates_first_write_writes_its_default_entry_without_a_point` (the entry was a
  point); the commit crate's BDD scenario "Concurrent first writes of a predicate both commit" (the second aborted on
  `Catalog`).
- **The per-run key-range skip (item 1).** An optimisation preserving behaviour: its suite
  (`conflict_history_skip_tests`, 6 tests at the bounds, inside them and past them, alone and after merges) passed
  against the code before the skip, as it must; its strength is the audit's planted defects (below).
- **Released proposals (item 3, 13:27).** 8 tests failed, no compile error: the four of `commit_step_release_tests`
  and `commit_step_store_tests::test_an_aborted_transaction_gives_its_buffers_back` at `StepOutput::released`'s
  `unimplemented!()`, and three earlier store tests through the driver, which drains the released proposals.
- **The pipeline (7.6, 13:31).** `run` stubbed with `unimplemented!()` over the written surface: 5 of the 6
  `pipeline_tests` failed at it (the sixth constructs and prints `PipelineError` only); the two BDD scenarios of
  "Acknowledgement at the visibility watermark" failed first at a nested executor in the steps (fixed by awaiting in
  async steps), then passed against the implementation.
- **Catalog changes and builds in rounds (7.7, 13:35).** The three scenarios bound ("A write after a build's start in
  the same round aborts", "A write executed against an old catalog aborts", "A write during an index build is in the
  built index") passed against 7a's resolver at their first run: a declaration's catalog entry is a conflict point
  inside every read set's catalog range, and the round's apply touches the build's nodes in round order. This is the
  "tested against the resolver, not assumed" the task asks for (decided 2026-10-27); no code changed for them.
- **Sessions (7.8, 13:46).** `Sessions::check` stubbed with `unimplemented!()` for a session other than 0: 5 of the
  then 8 `commit_step_session_tests` failed at it (the other three exercise registration, a failed sync and the error
  texts, which pass session 0 or none). Two then failed against the implementation on wrong expectations of the tests,
  corrected and recorded: a retry sent while its original is in flight is decided in the round after, once the
  original is applied, and is answered with its version (not busy); a refusal on arrival of a round closed at once
  replies at that round's sync.
- **The commit queue and the abort scenarios (13:47).** "A full commit queue answers busy" and "A write set over the
  maximum frame is refused" passed at their first run against the bound added for them; the rollup scenario ("A rollup
  writes between commit versions") passed against the sequencer at its first run, as group 5 built rollups to.

## 7.1 Scenarios

Bound verbatim, run by the crate's cucumber target `bdd` (18 scenarios, 36 steps), the commit step driven by the test
driver or, where the log matters, by the `Pipeline` over the write-ahead log on simulated storage:

| Spec | Requirement | Scenario | By |
|---|---|---|---|
| commit-rounds | Commits are grouped into rounds | both (7a) | driver |
| commit-rounds | Commit versions follow the clock | "A clock stepping back does not lower versions" (7a) | driver |
| commit-rounds | Validation is exact | both (7a) | driver |
| commit-rounds | The conflict history is bounded by the oldest open read | "A transaction older than the history aborts" (7a) | driver |
| commit-rounds | Acknowledgement at the visibility watermark | "No acknowledged commit is invisible", "A failed sync acknowledges nothing" | pipeline |
| commit-rounds | Catalog changes, drops and index builds are ordered in rounds | "Concurrent first writes of a predicate both commit", "A write after a build's start in the same round aborts", "A write executed against an old catalog aborts", "A write during an index build is in the built index" | driver |
| commit-rounds | Rollups take no version from the commit step | "A rollup writes between commit versions" | driver |
| transactions | Transactions are strictly serializable | "Write skew is refused" (7a) | driver |
| transactions | Exactly-once commits through sessions | "A retried commit commits once", "An evicted session is told so" | pipeline |
| transactions | Aborts name their reason | "A full commit queue answers busy" (pipeline), "A write set over the maximum frame is refused" (the worker's refusal, `TxnError::WriteSetTooLarge`) | |

**Not bound (remaining):** commit-rounds "A write into a dropped predicate aborts" (drops do not pass through the commit
step yet: `ChangesetApplier::drop_predicate` writes outside rounds) and the six scenarios of transactions "Commit-time
operations" (7.5's operations are not built; see Open questions).

## 7.2 Surface (this half)

| Item | What it is |
|---|---|
| `Pipeline<'a, L, S, C, E>` | The step on one node over an `OrderedLog` and a `ChangesetApplier`: `new(applier, log, bounds)`, `with_queue`, `with_sessions`, `commit(ticket, prepared, now_us)`, `register(ticket, now_us)`, `async sync(now_us)`, `oldest_read`, `replies_mut`, `buffers_mut` (every finished transaction's `TxnBuffers`), `needs_sync`, `is_stopped`, `watermark` (the store's applied version), `step`, `log`, `into_log` |
| `PipelineError(pub PipelineErrorEnum)` | `Log { cause: LogError }` (the log refused a round or its sync failed), `Apply { cause: ChangesetError }` (the store refused a synced round); each stops the pipeline |
| `Sessions` | The session table: `new(capacity)`, `DEFAULT_CAPACITY` (4,096), `check(session, request)`, `begin`, `committed`, `register` (the session evicted), `last`, `len`, `is_empty`, `capacity`, `bytes` |
| `Input::Register(ticket)` | A session registration, decided in a round; the session is numbered by the version it takes |
| `CommitStep::with_sessions`, `with_queue`, `sessions` | The table's bound; the commit queue's bound (busy past it); the table |
| `StepOutput::released`, `released_mut` | Every proposal the step will not apply, under its ticket |
| `CommitError` | Added `Busy { limit }` (retryable), `SessionEvicted { session }`, `RequestOutOfOrder { expected }`, `SessionBusy { session }`, with the wire's codes and details |
| `Executed::defaults` (store_apply) | The catalog keys an execution writes as a predicate's implicit default entry, sorted |

Dependencies added: `ultragraphdb_store_posting`, `ultragraphdb_traits_store` (the pipeline's bounds); dev:
`ultragraphdb_core_catalog`, `ultragraphdb_core_keys`. No external crate.

## Performance by design

| Choice | Why | Tested by |
|---|---|---|
| The per-run key-range skip: a run's point bounds are its first and last point (the sort gives them; nothing stored, nothing to go stale); its ranges' bounds stored, set by `fill` and `merge`; a cleared run's ranges bounds universal, so a bound left unset can only skip less | skips only, never decides; a defect in a bound degrades speed, not correctness | `conflict_history_skip_tests` (6), the model test; K1–K13 planted |
| Released proposals handed out in a reused vector of the output | an aborted transaction's `TxnBuffers` reach the next transaction: no allocation per abort | `test_an_aborted_transaction_gives_its_buffers_back` (capacities kept), `test_released_proposals_are_drained_by_the_caller_and_the_buffer_kept` |
| The pipeline keeps every finished transaction's buffers (`buffers_mut`) | the workers' steady state allocates no read set or record buffer | `test_the_buffers_of_applied_and_aborted_transactions_come_back`, `test_a_failed_sync_…` (3 back) |
| The session table a std `HashMap` (seeded hasher: client-chosen keys) of 40-byte entries, updated in place on each commit; eviction a scan for the oldest, paid only when a registration finds the table full | no allocation per commit; a registration allocates its entry and its record (rare) | `sessions_tests`; S1–S17 planted |
| Implicit default entries tracked per execution only when a predicate is first written; the installed catalog cloned only for an execution that wrote one, left alone for one that changed nothing | the common commit neither clones nor replaces the catalog (7a replaced it per execution) | `changeset_core_tests` (the three install tests) |
| No error built on the success path | — | review |

**Not allocation-free yet:** `apply_round` takes its round as a `Vec<(u64, Executed)>` by value, so the pipeline
allocates one vector per round (not per transaction); a `&mut Vec` there would remove it. A registration allocates its
record (`WriteSetRecord::encode`).

**The bench** (`history_bench`, as 7a ran it: 1 s warm-up, 3 s measurement, medians, the development Mac), before and
after the key-range skip, 2026-10-28:

| Per transaction | Window | Runs, before | Runs, after | `BTreeMap`s |
|---|---|---|---|---|
| Admit | 1,000 | 2.97 µs | 3.10 µs | 4.96 µs |
| Admit | 10,000 | 3.13 µs | 3.10 µs | 4.34 µs |
| Check, recent read | 1,000 | 1.71 µs | 1.76 µs | 4.48 µs |
| Check, recent read | 10,000 | 2.47 µs | 2.34 µs | 6.75 µs |
| Check, old read | 1,000 | 24.1 µs | 24.8 µs | 4.26 µs |
| Check, old read | 10,000 | 36.4 µs | 35.6 µs | 2.42 µs |

The skip changes nothing measurable: every difference is within run-to-run noise (±5%). The bench's keys are forward
lists of subjects drawn uniformly from a million, so every run of more than a few transactions spans nearly the whole
key space and no read key falls outside a run's bounds; the old read's cost is the binary search per read key in each
of the ~20 runs above it, which a bound cannot remove. The skip pays where keys cluster (a run of one namespace or one
predicate, the catalog range against runs of data, which it now skips outright) and costs two comparisons per key
otherwise. See Open questions.

## Phase 3: the audit

Each defect planted alone by `audit.py` in the session's scratchpad, restored in a `finally`, the `src/` trees of the
three crates hashed before the first plant and after the last (equal in each of the four runs); the build in a target
directory of its own. Suites run per plant: `store_apply` unit, then `txn` unit, then the commit crate's unit and BDD
(for a commit-crate plant, those two alone), stopping at the first that fails.

| Result | Count | Defects |
|---|---|---|
| Rejected at once | 49 | D1, D2, D4, D7, D8, D9; K1–K3b, K4b, K5, K7–K11, K13; R1–R5, R7; P1–P4, P6–P8, P10, P11; S1–S8, S10–S17 |
| Survived; a test added, planted again and rejected | 6 | below |
| Survived; the defect's behaviour taken as the code's | 1 | D6, below |
| Survived, equivalent | 1 | K6, below |
| Broken plants | none | two plants judged equivalent before running (an off-by-one making a skip more conservative) were dropped unplanted |

| Survivor | Closed by |
|---|---|
| D3, a catalog change installed without the default entries written since it read | `test_a_catalog_change_keeps_the_default_entries_written_since_it_read` |
| D5, an execution writing no catalog entry replacing the catalog with its own | `test_an_execution_writing_no_catalog_entry_leaves_the_catalog` |
| R6, a failed sync releasing admitted proposals under the tickets of every committed decision | `test_a_failed_sync_releases_each_admitted_proposal_under_its_own_ticket` |
| P5, a refused append not stopping the pipeline | `test_a_round_the_log_refuses_acknowledges_nothing_and_stops_the_pipeline` |
| P9, a stop leaving a round marked unsynced | the apply-refusal test now appends a next round before the refusal and asserts `!needs_sync` |
| S9, the session checked after validation | `test_a_retry_is_answered_before_its_reads_are_validated` |

**D6** planted the removal of the in-memory insert-if-absent (a default entry added to the installed catalog only
where it holds none). It survived because the resolver never admits a default entry after a declaration of its
predicate in one round; but `apply_round` can be called with both, and the store's batch then writes the default last,
so the defect's catalog agreed with the store and the code's did not. The code now adds the default as the batch writes
it (`test_the_installed_catalog_is_the_stored_one_after_a_default_follows_a_declaration`), and the inverted plant (D6r,
the check put back) is rejected.

**K6**, the ranges' bounds not set by a merge: equivalent. `merge` clears the run first, and a cleared run's bounds
are the universal ones (the empty start, no end), so a merge that forgot to set them would only skip nothing,
performance and not correctness; the choice is deliberate (a forgotten bound must never hide a conflict). The point
bounds, the ones that skip in practice, are derived from the sorted points and cannot go stale.

## Final counts

Equal under both build systems, 2026-10-28 14:00, Cargo (`cargo test -p`) against Bazel (`bazel test //<crate>/...`):

| Crate | Target | Cargo | Bazel |
|---|---|---|---|
| `ultragraphdb_transactions_commit` | unit | 110 tests | 110 |
| | bdd | 18 scenarios, 36 steps | 18, 36 |
| `ultragraphdb_store_apply` | unit | 130 tests | 130 |
| | bdd | 77 scenarios, 222 steps | 77, 222 |
| | workload_seeds, workload_faults, workload_replay | 1 each | passed |
| `ultragraphdb_transactions_txn` | unit | 62 tests | 62 |
| | bdd | 6 scenarios, 12 steps | 6, 12 |

The doc-test targets are empty or pass. `cargo build --workspace` is clean (once the client agent's transport crate
built again); `cargo fmt -p` and `cargo clippy -p … --all-targets` clean for the three crates (no `make format` or
`make fix`, at the orchestrator's word); `history_bench` builds under Bazel; `openspec validate
stage-5-transactions-and-binary-protocol` passes. `make test` is the orchestrator's.

## Phase 5

No decoder of bytes from a client, a peer or the disk was added: the session and request numbers are read through
`WriteSetRecordView` (fuzzed by `protocol_wire`), and a registration's record is encoded, not decoded. No fuzz target.
Mutation testing is 11.8's.

## Decisions taken here, for review

- **The implicit default entry as an insert-if-absent (item 4).** Realised as: the entry stays in the record's catalog
  section and is applied with the transaction, but is no conflict point, and the catalog the round installs merges it.
  The entry's value is fixed (`PredicateDeclaration::new(predicate)`, whatever the value written), so writing it twice
  is writing it once; and any other write of the key is a declaration, a catalog change that aborts every transaction
  of the namespace that read before it, so a default can never overwrite one through the commit step. No commit-time
  read of the store is needed for it, which is why it did not wait on the rest of 7.5. A namespace's initial catalog,
  written by its first write, stays a catalog change: concurrent first writes into a new namespace conflict (once) as
  before.
- **The catalog a round installs** is the one its store batch leaves: an execution that changed the catalog is
  installed with the predicates the installed catalog holds and it does not (defaults written since it read); one
  that only wrote defaults adds them to the installed catalog; one that wrote neither leaves it alone (7a's code
  replaced the installed catalog with every execution's own, which concurrent defaults would have made stale).
- **The pipeline withholds the replies of a round the store refuses** after its sync: the round is durable, so neither
  an acknowledgement (not applied) nor `Unavailable` (it will commit at recovery) is true; the pipeline stops, later
  commits are refused unavailable, and the client's session retry after the restart is answered from the table. The
  next round, appended but never synced, is answered unavailable.
- **The pipeline's watermark is the store's applied version**, which transactions begin at; a round whose last version
  is a registration's moves the step's version past it and the store's not.
- **Sessions are numbered by the commit version their registration takes**, unique and rising; a registration logs a
  write-set record of that session, request 0 and no writes, so a replay can tell it. Session 0 is no session (the
  commits of tests and of internal writers).
- **The order of checks** is the Lean model's: the record parsed (sessions need its fixed part), then the session (a
  retry answered with the stored version, released, no decision in the round; evicted, busy or out of order refused as
  aborts in the round), then validation, then a version. A session's committed request is recorded in the table once
  its round is applied, and marked busy from its admission until then: a second request of it, in the same round or the
  next while the first is in flight, is `SessionBusy`; a retry decided after the original is applied is answered.
- **An unknown session reads as evicted** (`SessionEvicted`), as TigerBeetle answers a client its table does not
  hold: the client's recourse, registering again, is the same.
- **Eviction** takes the session touched (registered or committed) longest ago, one with no request in flight first.
  The default bound is 4,096 sessions (160 KiB of entries), configurable with `with_sessions`.
- **The commit queue** is bounded by `with_queue`, unbounded by default so the node (10.3) sets it; a commit to a full
  queue is `Busy`, retryable, its detail zero as the wire's table states. Registrations are not bounded by it.
- **The session errors are not retryable**, as the wire's code table marks them.
- **Released proposals** leave in the step that decides so, under their tickets: an abort when its round closes, a
  retry when it is answered, a refusal on arrival, and the round in flight and the requests waiting on a stop.

## Open questions for the user

- **7.5's commit-time operations are not built, and three decisions block them.** (a) *Where the outcome is logged.*
  The write-set record has no section for an operation, and a replay applies records, not executions, so the writes an
  operation makes (the created node's properties, its links, the counter's new value, the compare-and-set's) must be
  in the round's log record: a second record per operation transaction, the transaction's record rewritten with them
  (a copy per such transaction), or a new section of the record (a wire change, group 9's layout). (b) *How the store
  at W plus the decided but unapplied writes is read.* No read path in `store_posting` stacks unapplied `Writes` over
  a snapshot (stage 5 removed `StackedLayers` from transactions); the operations need one, bounded to the round in
  flight and the current round. (c) *How a transaction names an operation and how the reply carries its result* (the
  UID and whether it was created; the value a failed compare-and-set found): `Transaction` has no operation API,
  `Prepared` no operation, `Reply` only a version, `CommitError` no `OperationFailed`. Which of (a)'s three, and is a
  read overlay in `store_posting` (b) the intended home?
- **Two builds in one round.** Two declarations in different namespaces, each starting an index build, are both
  admitted (no common key), and `apply_round` then refuses the whole round (`ChangesetError::Rebuilding`): the pipeline
  stops with the round durable. Should the commit step refuse a second build-starting transaction in a round, and one
  executed before a build it does not know of started (the conflict is on no key the resolver holds)?
- **Drops in rounds.** `drop_predicate`, `drop_graph`, `drop_data` write outside the commit step, so the resolver never
  sees their ranges and "A write into a dropped predicate aborts" cannot be bound. The commit-rounds requirement also
  says a drop aborts a transaction that *wrote* in its range; the resolver checks written ranges against reads only, so
  a blind write (no read of the range) would not abort. Is the second half meant?
- **The key-range skip measured nothing.** The approved skip is in, exact, and costs nothing measurable, but the old
  read is unchanged (35.6 µs at a window of 10,000). Its cost is a binary search per read key in each of ~20 runs. What
  would reduce it keeps validation exact but changes the layout: fewer, larger runs (a higher cap, paying larger
  merges), or checks moved to the workers (design.md's risk "the commit step becomes the ceiling"). Wait for group 12's
  share of old reads?
- **Session persistence and recovery (7.8's keyspace, 7.9).** The table lives in the step only. Keeping it "in the
  keyspace without versions, written in the round's batch" needs a session key layout (a tag of the keyspace without
  versions in `core_keys`, whose decoder takes a fuzz target) and an `apply_round` that writes extra entries; replay
  above the store's applied version needs a store path that applies a round's records (raw deltas, dictionary,
  statement and catalog entries, leases moved past the UIDs they name, builds restarted) rather than executions. Both
  are store-level work in `store_apply` and `store_posting`; confirm the direction before it is built.

## What remains

- 7.5 (above), and with it 7.1's six "Commit-time operations" scenarios and 7.2's operation surface.
- 7.7: drops in rounds and "A write into a dropped predicate aborts"; the two-builds question.
- 7.8: the table in the keyspace without versions (the in-memory table, registration through a round, bounding,
  eviction and retries are built).
- 7.9: recovery.

## What group 8 should expect

- The commit step records a vector build's touched nodes as 7a left it: `apply_round` takes each execution's
  `touched` in round order while a build runs; the scenario "A write during an index build is in the built index" is
  bound for a term index. A vector index build runs the same path.
- A declaration starting a build commits through the step like any catalog change, and aborts its namespace's
  concurrent writers on `Catalog`; see the two-builds question.

## What group 10 should expect

- Drive `Pipeline`: `commit` and `register` per request, `sync` whenever `needs_sync()` (the future completes on the
  compio runtime's completion), `oldest_read` from the workers' open transactions, then drain `replies_mut` (route by
  ticket) and `buffers_mut` (lend to workers). A `PipelineError` means the node refuses writes until it restarts.
- Set `with_queue` (the commit queue's bound, answering `Busy`) and `with_sessions`; the session layer reads a
  session number from a registration's reply and sends it with request numbers from 1 in each commit's record
  (`Transaction::commit(session, request, …)`).
- `CommitError::code` and `detail` are the wire's for every reason, the session errors included.
- Recovery is not built: a node opened over a log ahead of its store must not start the pipeline yet.

## Outside the scope, noticed

- `ultragraphdb_transactions_txn/tests/types/read_set/read_set_tests.rs` showed a reformatting (`drop(…)` arms
  rewritten as blocks) that this group did not make; left as found.
- While this group verified, the workspace failed to load (`two packages named ultragraphdb_protocol_transport`):
  the client agent's work in progress, not this group's.
