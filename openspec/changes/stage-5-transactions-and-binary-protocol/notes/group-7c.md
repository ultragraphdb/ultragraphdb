# Group 7c: commit-time operations, drops in rounds, one build a round, persisted sessions and recovery

`ultragraphdb_transactions_commit`, with changes in `ultragraphdb_store_posting`, `ultragraphdb_store_quads`,
`ultragraphdb_store_apply` and `ultragraphdb_transactions_txn`. The user's decisions of 2026-10-28 on 7b's open questions
are implemented as stated.

## Phase 2: the failing runs

- **Commit-time operations (7.5, 2026-10-28 15:01).** Written against the surface with `unimplemented!()` bodies, no
  compile error:
  - `store_posting` unit: 261 tests, 2 failed, at `Batch::stack` and `Request::seed_deltas`.
  - `store_quads` unit: 25 tests, 1 failed, at `QuadWrite::seed`.
  - `store_apply` unit: 140 tests, 9 failed, every one of `changeset_core_operate_tests` at `ChangesetCore::operate`
    (the error test of the new variants passes: the constructors and texts were written whole).
  - `txn` unit: 65 tests, 2 failed, at `Transaction::operate`.
  - commit unit: 117 tests, 14 failed: 7 at `ApplierOperator::begin_round` (every pipeline test closes a round
    through it), 4 at `Reply::with_results`, 2 at `CommitError::code`'s new arms, 1 at `NoOperations::operate`;
    one deferral test first failed on its own sizes (a round that held both records), corrected, then at the stub.
  - commit bdd: 24 scenarios, 11 failed: the six "Commit-time operations" scenarios at `Transaction::operate`, the
    five pipeline scenarios at `ApplierOperator::begin_round`.
- **Drops in rounds and one build a round (7.7, 15:20).** Written after their code, not before: the time box went to
  7.5 first. Their tests (`test_a_write_into_a_range_written_above_the_read_version_conflicts_as_a_drop`,
  `test_a_blind_write_into_a_range_dropped_after_its_read_aborts`, `test_a_round_closes_after_a_drop`,
  `test_a_round_holds_at_most_one_index_build`, the store tests of `drop_prefixes_at` and `prepare_drop_predicate`, and
  the scenario "A write into a dropped predicate aborts") were each seen to reject its rule's removal in the audit
  instead (C6–C9, A11–A13, P4, T4 below), which is the evidence phase 2 would have given. Two failed on their own
  expectations first and were corrected: a transaction creating an IRI another created concurrently aborts on its
  absence (the rule of new IRIs), and a transaction of the dropped predicate's namespace that read below the drop aborts
  as a catalog change aborts it (the drop's range holds the predicate's catalog entry).

## 7.1 Scenarios (this part)

Bound verbatim; the crate's `bdd` target now runs 25 scenarios, 50 steps:

| Spec | Requirement | Scenario | By |
|---|---|---|---|
| transactions | Commit-time operations | all six: "Concurrent insert-if-absent yields one node", "Concurrent upsert-and-link deduplicates and links atomically", "Properties for creation are written once", "An upsert-and-link in an aborted transaction writes nothing", "A failed compare-and-set writes nothing", "Counter adds commute" | pipeline over the log: the first commit alone, half the rest while it syncs, the others while that round syncs, so every operation runs over a round in flight and its own round's admitted proposals |
| commit-rounds | Catalog changes, drops and index builds are ordered in rounds | "A write into a dropped predicate aborts" | driver |

Every scenario of `commit-rounds` and of the four `transactions` requirements 7.1 names is now bound. The
`write-ahead-log` scenarios about recovery (7.9) are not: see What remains.

## 7.2 Surface (this part)

| Item | What it is |
|---|---|
| `Operator<P>` (commit) | What runs a proposal's operations for the step: `begin_round`, `operate(proposal, unapplied, results)`, `staged_record`, `adopt`, `admitted`; type `Staged` |
| `ApplierOperator<'a, S, C, E>` | The node's operator on the changeset applier, holding the round's overlay (`Batch`), built at its first operation; `new`, `applier`, `has_overlay` |
| `NoOperations` | The operator of a step without one: every operation refused (`Refused`) |
| `CommitStep::step_with(input, now_us, out, operator)` | `step` is `step_with` over `NoOperations` |
| `Proposal::has_operations`, `starts_build`, `is_drop` | Defaults false; `Prepared` answers from its execution |
| `Reply::with_results`, `results` | The operations' results, the wire's `OperationResult`, in asked order |
| `CommitError` | Added `OperationFailed { found }` (code OperationFailed, not retryable, `found()` the N-Triples value) and `Refused { reason }` (code Refused) |
| `Resolver::check_writes`, `ConflictHistory::written_into_range` | A key written, whole or as a statement's list, inside a range written above the read version: `Conflict(Drop)` |
| `Transaction::operate(operation, now_us)` (txn) | Records an operation; refused read-only, ended or past its limit |
| `Prepared::operations`, `has_operations`, `operated(applier, overlay, pending, results)`, `adopt`, `drop_predicate(applier, ns, predicate, buffers)`, `is_drop`, `starts_build`; `Operated` | The transaction as it commits once its operations ran, its record written again |
| `Operation`, `OperationLink` (store_apply) | The four operations in oxrdf terms; a link `from_node` / `to_node` |
| `ChangesetApplier::operate(executed, operations, overlay, pending, results)` | Runs the operations on top of the execution; the execution as it commits |
| `ChangesetApplier::prepare_drop_predicate`; `Executed::is_drop`, `drop_ranges`, `starts_build` | A drop as a proposal; `apply_round` drops it at its version after the batch |
| `ChangesetError::OperationFailed { found }`, `NotUnique { predicate }` | |
| `QuadWrite::seed(writes, terms)`, `WriteTerms::know(iri, uid)` (store_quads) | A write continuing a finished one; an IRI known by an unapplied creator's UID |
| `Batch::stack(writes)`, `Request::seed_deltas`, `Request::add_versionless_entry`, `Lists::drop_prefixes_at(ts, …)` (store_posting) | The overlay's layers; a request's own blind deltas; a removal kept; a drop at the step's version |

No dependency added but `ultragraphdb_core_terms` to the commit crate's dev-dependencies. No external crate.

## Performance by design

| Choice | Why | Tested by |
|---|---|---|
| The overlay is the posting lists' stacked layers (`Batch::stack`), built at a round's first operation from the round syncing and the round's admitted proposals, then each admitted proposal stacked as it is admitted, dropped when the next round begins | a round without operations stacks nothing and allocates nothing for them; the overlay holds at most two rounds, bounded by the round bounds (1 MiB of records a round) | `test_the_operator_sees_the_round_syncing_and_the_rounds_admitted_before_it`; C3, C4 planted |
| An operation's write is seeded with the transaction's (`QuadWrite::seed`), its deltas the request's blind writes, so the two leave one delta per list through the lists' own scratch layer; statistics deltas merged | no posting-list merge written beside the list's; a copy of the transaction's writes per operation transaction, none for any other | `test_seeded_deltas_are_the_requests_own_writes`, `test_a_seeded_write_continues_the_write_it_was_seeded_with`; P1–P3, Q1, Q2 planted |
| The record is written again only for an operation transaction (`WriteSetBuilder` into a fresh buffer); the commit record is copied at commit only when operations were asked | a transaction without operations is unchanged on the hot path: no copy, no allocation | `test_a_transaction_without_operations_asks_none` |
| The IRIs unapplied proposals created are passed as known (`WriteTerms::know`), gathered per operation transaction | exact without a dictionary layer: an IRI is never given two UIDs | the upsert-and-link scenario; A8 planted |
| Results kept per ticket only for operation transactions; reply results an empty `Vec` (no allocation) otherwise | — | `test_a_reply_carries_its_operations_results` |
| The written-keys check walks only runs holding a version above the read version and skips a run with no range in one comparison (`range_bounds`) | drops are rare: the common run holds no range, and its check is one `Option` test | `test_a_blind_write_into_a_range_dropped_after_its_read_aborts`; C6, C7 planted |
| No error built on the success path | the refusals (`OperationFailed`'s found text, `Refused`'s reason) are built only when an operation is refused | review |

**Not allocation-free:** an operation transaction allocates its seeded write (a clone of its terms, catalog and
deltas), the pending IRIs' vector and its new record; the overlay allocates its layers (`Arc<PostingList>` per key)
once a round has an operation. Measured nothing: no close choice was left that a bench would settle, and the old
read's layout waits for 12.3 (decision 4).

**Differential testing.** The overlay is not tested against a separate plain model: its model is the serial one the
scenarios state, a hundred counter adds over three rounds ending at the start plus 100, eight insert-if-absents over
three rounds leaving one node, each against what a fresh transaction reads back at the watermark. A seeded property
test interleaving the four operations against a serial model is left for 11.3's node simulator, which runs workloads
from a seed.

## Decisions taken here, for review

- **Where the operations live.** `Operation` and `OperationLink` are `ultragraphdb_store_apply`'s, beside the changeset
  write that executes them, in oxrdf terms; the session (10.2) converts the wire's `CommitOperation` to them as it
  converts a Write body to a `Changeset`. `ultragraphdb_core_mapping` would also fit; no second crate names them yet.
- **The operations' write continues the transaction's** (`QuadWrite::seed`): the transaction's terms, its list
  deltas as the request's own blind writes and its statistics merged, so the two leave one write: one delta per list,
  one statistics delta per predicate, the transaction's dictionary entries once. The result replaces the proposal's
  execution and record only once its record fits the round; else the proposal waits for the next round and is decided
  again there from the start (its leases' numbers spent, as a refused write's are).
- **What an operation reads.** The store at the watermark with the round syncing and the current round's admitted
  writes stacked above it, through the request's read (the posting cache included); the IRIs those proposals created
  are known by their UIDs (`WriteTerms::know`), since no layer holds dictionary entries. Without that, the second of two
  operations naming a predicate new to the store gave it a second UID; found by the upsert-and-link scenario. A literal
  whose string entry an unapplied round writes is not decoded from a layer: a compare-and-set or counter on such a
  value is refused (`Refused`, the store's error) rather than read; see Open questions.
- **A node's kind for the statistics.** A link's node is decoded for its kind (IRI or blank); a node whose dictionary
  entry is in an unapplied round is counted as a blank node, which every node an operation creates is.
- **Insert-if-absent finds the smallest UID** holding the value when several do (a unique value held twice is a
  violation the catalog prevents; the choice only has to be deterministic).
- **Compare-and-set** holds when the subject's values of the predicate are exactly the expected one; none held is
  refused with no value found, several with the first. **Counter add** reads the value at commit, exactly (the step is
  serial), adds within `i64` and writes `xsd:integer`; a value that is no integer, several values, or an overflow
  refuse the transaction as a failed compare-and-set does (`OperationFailed` with the value found).
- **Results.** The reply carries the wire's `OperationResult` for each operation, in order; compare-and-set and counter
  add give zero and `false`, as the wire states. A counter's new value is not returned: the wire's result has no field
  for it (the brief named one; see Open questions).
- **Refusals.** An operation that did not hold is `CommitError::OperationFailed { found }` (the wire's code 8, not
  retryable, the value as the Error frame's value); one the catalog or store refused is the new
  `CommitError::Refused { reason }` (code 7, the spec's "refused, when the catalog refuses a write"). A step without an
  operator refuses every operation as `Refused`.
- **A drop closes its round**, so it is applied after the round's batch at the round's last version, and needs no
  version between two of the batch's; `Lists::drop_prefixes_at` drops at that version. A drop is one predicate's
  (`Prepared::drop_predicate`); graph, data and namespace drops keep their direct paths (no scenario asks them in
  rounds yet).
- **A drop's range holds the predicate's catalog entry**, which the physical drop removes, so it overlaps every
  read set's catalog range: a drop aborts its namespace's transactions that read below it, as a declaration does, with
  `Conflict(Drop)`. The per-graph ranges are the predicate's keys in each graph holding it when the drop is prepared;
  a graph first written after that is caught by the catalog range and dropped by the apply, which expands the prefixes
  again after the batch.
- **One build a round**: the round closes before a proposal that starts a build once one admitted in it does; a refused
  one does not count.

## 7.8 Sessions persisted

- **Where.** `SessionEntry` (commit crate) at `\xffultragraphdb/session/` + the session u64 big-endian, and the index of
  the sessions held at `\xffultragraphdb/sessions`, ascending u64 little-endian: under the system prefix, outside every
  namespace, so no drop removes them, and with no change to the six `Store` implementations (no prefix scan of the
  keyspace without versions is needed).
- **What.** Each entry holds the whole reply, `request` u64, `version` u64, the result count u32, 4 zero bytes, then
  16-byte results as section 9 and the Commit reply hold them; the decoder refuses every malformed entry as
  `StoreError::Corrupted` naming its key, and `fuzz/ultragraphdb_transactions_commit/` has its target, seeded from
  `testdata/fuzz_seeds/session_entry/` (not run: fuzzing is 11.7's).
- **When.** `Sessions` keeps what changed since its last drain; the step drains it into `StepOutput::system` when a
  round is applied, and the pipeline writes those entries in the round's batch through `apply_round_with(round,
  system, last version)`, so a round of registrations alone writes its entries and moves the watermark to its version
  (before, the store's applied version lagged a trailing registration, and a restart could have given its version
  again). A round ending in a drop writes them after the drop, which must land above the watermark.
- **Open.** `Pipeline::new` loads the table and returns a `Result` (`PipelineError::Sessions`). A loaded table holds
  at most the default capacity or what it holds, whichever is more; `with_sessions` bounds it, its evictions written
  with the first round applied.
- `Lists::commit_round_with`: a `through` below the watermark is refused; one at the watermark writes the system's
  entries alone and leaves it.

## Every write through the step

The bug, found reading the code: a build's swap and undo committed at the store's own next version, and so did the
physical removal of a removed index and a rebuild record's removal. Under the pipeline the step's next round took the
same version, and its apply was refused as stale, which stops the pipeline. A build-starting declaration admitted
while a build ran was refused at apply the same way.

- **Protocol first.** A write-set record's section 10 names its operation (`RecordOperation`: drop a predicate, swap
  a build in, undo a build, with the predicate's UID or the build's record number), so the log says what a round the
  store computes does, and recovery can redo it (`PROTOCOL.md` §12.2). The golden Round regenerated (5,776 bytes;
  records 280 and 5,136), the wire's `write_set_record` and `round` fuzz seeds with it, which were stale since before
  section 9.
- **Cleanups take no version.** `Lists::drop_prefixes_at_watermark` and `Lists::remove_rebuild_record` write at the
  watermark: an index no catalog names is read by no read of the watermark's catalog.
- **Sequenced builds.** `ChangesetApplier::sequence_builds`, which `Pipeline::new` calls: the build job keeps what it
  made and ends with `BuildEnd::Swap`, or with `BuildEnd::Undo` when it failed or a drop cancelled it; a build the
  store failed under proposes nothing. `prepare_build_end` and `Prepared::end_build` make the proposal; its round's
  apply swaps or undoes at its version, building again first when nothing is kept (what recovery will need), and a swap
  whose build a drop cancelled meanwhile is discarded. A round's apply now always ends with the watermark at its last
  version.
- **The step.** `Proposal::ends_build`; a build runs from its start's admission to its end's; a proposal starting
  another waits, set aside (`CommitStep::deferred`), and goes back to the queue's front once the end is admitted; an end
  is a round of its own; a failed sync answers the set-aside unavailable. `with_build_running` for a store opened with
  one. This replaces 7c's "a round closes before a second build": `test_a_round_holds_at_most_one_index_build` became
  `test_a_second_build_waits_for_the_first_builds_end`.
- **The pipeline.** `poll_builds` proposes every finished build's end under `Pipeline::SYSTEM_TICKET`, whose replies go
  to nobody; `sync` calls it after each round; `PipelineError::BuildEnd` when one cannot be proposed.
- **Not done.** The drops of a graph, a namespace's data, a namespace and everything stay direct-only: the wire has no
  drop command (`PROTOCOL.md` open point 28), so no client reaches them through the node.
- Tests written before the code and seen failing (compile errors or the assertion): the wire's three
  `record_operation_tests`, the builder's section 10, `Prepared::end_build`, `store_apply`'s four sequenced and
  watermark tests, the step's three build tests and the pipeline's swap test. The posting lists' cleanup test was
  written with its code, not before; the audit covers it.

## Phase 3: the audit

34 defects over 7.5 and 7.7, each planted alone in the tree and restored in a `finally`, the five crates' `src/` hashed
before and after (equal). The agent ran it; after the session restarted mid-run, the orchestrator found P3 still
planted (its anchor had been corrected and the rerun killed before its `finally`), restored it, matched the tree to the
audit's original hash, and closed the survivors.

| Result | Defects |
|---|---|
| Rejected at once by a test aimed at the rule | 30: P1, P2, P4, Q1, Q2, A1–A7, A9–A13, T1–T4, C1–C3, C5–C10 |
| Broken plant (anchor matched nothing), corrected and rejected | P3, a seeded delta not taken as blind: `test_seeded_deltas_are_the_requests_own_writes` |
| Survived; a test added, re-planted and rejected | A8, the unapplied proposals' created IRIs not taught to the operation: `test_an_operation_on_an_iri_an_unapplied_proposal_created_reuses_its_uid`; C4, the overlay kept into the next round, which reads an applied round's writes twice: `test_the_overlay_lives_one_round_and_reads_the_unapplied_proposals` |
| Equivalent | Q3, `know` overriding an IRI the write itself created: both would be creators of one new IRI, and the resolver aborts the later on the IRI's recorded absence before any operation runs |

## Open questions for the user

- **A build across rounds stops the pipeline.** Decision 2 keeps two builds out of one round, but (a) a second
  declaration starting a build while the first still runs is refused by `apply_round` (`Rebuilding`, one build at a
  time) after its round is durable, stopping the pipeline; and (b) a build's swap commits through `Lists::commit` at a
  version of its own (`changeset_core_rebuild.rs:363`), outside the sequencer, so the next round's first version can be
  at or below it and `apply_round` refuses it (`StaleVersion`), stopping the pipeline. Found writing
  `test_a_round_holds_at_most_one_index_build`, which stops before the second build's apply for that reason. The direct
  drops (`drop_graph`, `drop_data`, `drop_namespace`, `drop_all`) and `ChangesetApplier::apply` take versions the same
  way. Should the swap be a proposal of the commit step, as the predicate drop now is, and a build-starting declaration
  wait in the step while a build runs (the pipeline knows `applier.rebuilding()`), or be refused at execution?
- **A retried operation transaction's results.** The session table stores a committed request's version, so a retry
  of an operation transaction is answered with its version and no results. Store the results in the table (bounded,
  16 bytes each), or have the client read them back?
- **A counter's new value in the reply** needs a field in the wire's `OperationResult` (group 9's layout); the brief
  asked for it, the wire has none.
- **Dictionary strings in the overlay.** A long literal's string entry an unapplied round writes is not read by an
  operation (no layer holds versionless entries), so a compare-and-set or counter add on such a value is refused rather
  than read. Rare (only strings stored out of line, written in the last two rounds); a versionless layer in the
  overlay would close it.

## What remains

- **7.9, recovery**, not started. The step opens at the store's applied version; the log's rounds above it are read in
  order and their committed records applied through a store path (`apply_records`: the records' sections as one batch
  at their versions, leases moved past the UIDs and statement IDs they name, catalogs reloaded, drops performed, a
  build's record left to `finish_records`), and the session entries rebuilt from the records (session, request,
  version, section 9's results; registrations from request-0 records). A store ahead of the log's tail, or a round
  missing between them, is refused naming the version. Its scenarios in `write-ahead-log` ("Rounds lost from the store
  are replayed", "A store ahead of the log refuses to start", "A round never reaches the store before the log", "A
  reported fault loses no acknowledged round") are not bound yet.
- The two build questions above.

## What groups 8 and 10 should expect

- **Group 10** drives `Pipeline` as before; operations need nothing more: the pipeline's operator runs them. Convert
  the wire's Operation body to `ultragraphdb_store_apply::Operation` (terms through the term table, `Link` to
  `OperationLink`), call `Transaction::operate` per operation, and send `Reply::results()` in the Commit reply's
  results. `CommitError::found()` is the Error frame's value for `OperationFailed`. A drop is
  `Prepared::drop_predicate(applier, namespace, predicate, buffers)` handed to `Pipeline::commit` like a commit.
  Recovery is not built: a node opened over a log ahead of its store must not start the pipeline yet; and a build
  completing while the pipeline runs stops it (Open questions).
- **Group 8** (the vector indexer): an operation's writes go through `ChangesetWrite`, so a vector an operation's
  properties carry is written as a changeset's is and reaches the indexer by the same path; while a vector index is
  built, the operations' touched nodes are merged into the transaction's.

## Outside the scope, noticed

- `src/types/reply.rs` now holds more than 25 lines and would move to `reply/mod.rs` by the layout rule; left as a
  file to keep the diff and the tests' paths small.
