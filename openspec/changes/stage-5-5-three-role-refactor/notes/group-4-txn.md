# Group 4: the commit step and the transaction (`transactions_commit`, `transactions_txn`)

Agent 3 of group 4's four. Scope: `ultragraphdb_transactions_txn`, `ultragraphdb_transactions_commit` and the fuzz
package `fuzz/ultragraphdb_transactions_commit`. Logs and scripts are in the session scratchpad (`g4txn/`).

## 4.1 The Ultra Style lift

Neither crate compiles at `HEAD` (group 2 and 3 moved `core_keys`, `core_terms`, `store_quads` and `store_apply`, and
`protocol_wire` no longer builds on `core_terms`), and `95462282` does not build `core_algo` (its `uint64_heap` module
is named and missing). Both crates were therefore lifted in a scratch worktree at `dfd73680`, as group 3 lifted the
store crates: neither crate, nor the fuzz package, changed between `dfd73680` and `HEAD`. The diff (`g4txn/lift.diff`,
14 files, +103 −19) was applied to the main tree unchanged.

Bazel before and after, the same counts:

| Crate | Unit | BDD |
|---|---|---|
| `transactions_txn` | 78 | 6 scenarios, 12 steps |
| `transactions_commit` | 174 | 37 scenarios, 74 steps |

`cargo fmt -p` and `cargo clippy -p <crate> --all-targets -- -Wclippy::all` report nothing after. Review of the rules no
lint checks:

- **Rule 1.** No function of either crate calls itself or reaches itself through another.
- **Rule 2.** `Run::merge_points` and `merge_statements` were `loop`s; each is now a `for` bounded by the two runs'
  entries, every turn taking an entry of one or both. The other loops each carry a comment naming why they end:
  the round's gathering (`CommitStep::close`, every turn takes, sets aside or ends), the return of proposals set aside,
  the conflict history's merge and prune (each turn removes a run), the session table's bound (each turn evicts one),
  the binary search for the largest record (`RoundBounds`), the pipeline's build ends, and recovery's log scan (each
  read starts after the last round read; a debug check that a read returns rounds from its position).
- **Rule 3.** The commit step and the transaction are data plane. Allocations left: a round's registration records,
  the operations' result vectors and the session table's map (allocating at a registration, never at a commit) were
  already so; the write-set builder's table entry, which the lift first wrote through a temporary `concat`, writes in
  place. Nothing new allocates per commit.
- **Rule 5.** Debug checks added: the read set's catalog end exists, its arena stays within u32 offsets, sealed tables
  are strictly sorted; the builder's record has its fixed part and table and is exactly its computed size within its
  bound; a transaction reads at or below the watermark and its execution at its read version and tenant; the
  sequencer's versions are even and strictly increase; a round record commits at most its decisions; a round's body
  is exactly the sections its table names and its sources are its commits; a merged run holds no more entries than
  both; the largest-record search's result fits and the next does not; a session entry's encoding is its decoded bytes.
- **Rule 7.** The read set's catalog bounds (`prefix[0]`, `prefix[1..]`, `next[last] += 1`), the builder's section
  table slot (`slot[..4]`), the session entry's results (`r[8]`, `r[9..]`, `r[..8]`, now one destructuring of the
  16-byte array) and the session index's order (`w[0] >= w[1]`, now `is_sorted_by`) read without indexing.
- **Rule 8.** No `cfg` in either crate's `src/`.
- **Rule 9.** No `Arc`, `Mutex`, `RefCell`, `Cell`, atomic or trait object in either crate's `src/`. The one exception
  in place, `#[expect(clippy::large_enum_variant)]` on `Commit`, predates this change and carries its reason.

## 4.2 Scenarios bound

`transactions_commit` (58 scenarios, 116 steps): `features/commit-rounds/commit_rounds.feature` ("Commit versions are
built from the epoch", all four, replacing the removed "Commit versions follow the clock"; "Acknowledgement at the
visibility watermark", both in the new text; "A round closes at a delivered close", of the catalog requirement, because
the round closing at a close is this group's), `features/sequencer/partitions.feature` (new: "Partitions learn each
closed epoch and stamp the open one", all four, from the partition's side), `features/transactions/transactions.feature`
(rewritten: snapshot's "A snapshot read waits for its epoch", strictly serializable, what reads record (all six),
exactly-once (four of six), aborts (all three), commit-time operations (all nine), session tokens (all three)), and
`features/write-ahead-log/recovery.feature` ("A store ahead of the log" and "A store behind its checkpoint" in the new
position text, at the coordinator's request). `transactions_txn` (6 scenarios): the snapshot requirement's three
worker-side scenarios in the new text and "Interactive transactions".

Halves bound here, the rest elsewhere, each step commented: "An evicted session is told so" and "A session belongs to
its principal" are the frontend's owner check (group 9) and are not in the feature; a snapshot read's wait is bound on
`CommitStep::closed()` (the read is compute's, group 9); "A read with a token sees the commit" binds the partition's
watermark; "Inserts of the same new statement conflict" binds the partition's decision (registration at the home is
group 7's); "Creating a dictionary entry conflicts with nothing" stands the home's registration in by
`Input::Registration`; the purge's eviction comes from the registry (group 6) carried by a close (group 5); a
reconnecting partition takes the missed closes in order (the sequencer's catch-up is group 5's). The two old stage-5
catalog scenarios ("A write after a build's start…", "A write executed against an old catalog…") are rebound on catalog
versions announced by a close, since catalog keys no longer conflict; group 6 replaces their texts.

The step definitions of the commit crate's BDD target were adapted and written by a subagent from the unit tests and
the features against the phase-1 surface, before the implementation.

## 4.3/4.4 Phases

Phase 1 surfaces (bare `unimplemented!()`): `EpochStamp` (replacing `Sequencer`; `peek` added in phase 2 with its test),
`CloseRequest` and `SettingError`, `Input::{Registration, Close, Tick}` (no `Register`: sessions are the metadata
group's), `StepOutput::{position, round_ref(parent, derivation), close_request}`, `Reply::{with_token, token, terms}`,
`StoredReply`, `Sessions` over `u128`, `SessionEntry` (16-byte keys, token, results and node texts), the
`CommitStep` builders and accessors (`with_delivered`, `with_partition`, `with_tail`, `with_close_request`,
`with_catalog`, `delivered`, `open`, `closed`, `catalog_version`, `tail`, `held`, `fault`), `CommitError`
(`CloseOutOfOrder`, `CrossPartition`; `RequestOutOfOrder { last }`, `SessionBusy { session: u128 }`;
`SessionEvicted` and `PermissionDenied` removed, the frontend's), `RecoveryError` (`Derivation`, `AppliedPosition`;
positions in `StoreNotInLog`, `Gap`, `BehindCheckpoint`), `Replay::{run, run_in, applied_position,
APPLIED_POSITION_KEY}`, `Pipeline::{with_partition, registration, close, tick, take_close_request, retry}`,
`ApplierOperator::bound`, `Proposal::{token, dictionary}`; in `transactions_txn` `TxnError::cross_partition`,
`ReadSet::{term_key, TERM_TAG}` and absent terms by ID, the catalog range removed, `WriteSetBuilder` over the wire's
logical `WriteSetRecord` (`build`, `encode_into`, `dictionary_len`), `Transaction::{with_catalog, with_token, bind}`,
`Prepared::{token, dictionary}`; in `store_apply` `LogicalQuad`, `LogicalWrites`, `Executed::logical`,
`ChangesetApplier::prepare_logical`; in `core_ids` `Bucket::of_value`.

Phase 2 (logs `g4txn/*phase2*.log`): `transactions_txn` unit 87 tests, 79 failed, every one `not implemented` from the
surface (8 passed: the unchanged error and limits tests); BDD 6 scenarios, 6 failed `not implemented`.
`transactions_commit` unit 207 tests, 184 failed: 182 `not implemented` from the surface, 2 the largest-write-set
assertions on the round's new four-section table; BDD 58 scenarios, 57 `not implemented`, 1 failing on the stand-in for
`Bucket::of_value`, then absent. `store_apply` 207 tests, 5 failed, `not implemented` (the new ones).
`Bucket::of_value`'s test was written with its golden values (python-xxhash 3.6.0) before the code but not run failing
first.

Phase 4 corrections to tests, each test data and none a weakened assertion: replies now carry session tokens (old
comparisons strip them through `fixture::plain`, tokens tested on their own); version literals moved to the epoch layout
(`version(epoch, sequence)`); `rdf:type`'s entries in a tenant's first write (8 dictionary entries, not 6); a new
predicate's ID read from the execution, not the dictionary; the write set now counts the dictionary entries its round
holds; a too-old read version in the epoch tests' proposals; a retry decided after its first is applied answers the
stored reply (not busy); the drop scenario's unrelated write commits (no catalog range); a doubled conflict is named
`Statement` before `AbsentTerm` by the spec's order; the round's largest record is 1,046,216 / 261,320 (8 bytes less,
the section table of four); a store refusing its batch is now provoked by a fault (versions no longer collide); the
simulated store keeps a prefix of batches at least as long as the last sync, so recovery tests bound the position
instead of naming it. Implementation fixes found by the suite: `Sessions::bounded` keeps the sessions it holds.

Final Bazel: `transactions_txn` unit 87, BDD 6 scenarios / 12 steps; `transactions_commit` unit 208, BDD 58 scenarios /
116 steps; `store_apply` unit 207 (BDD 85 scenarios and the three workloads green); every `ultragraphdb_store/...`
target, `tool_test`, `core_ids` and `protocol_wire` green (31 targets).

**Differential replay check (4.4).** `store_apply`'s `test_replay_derives_every_key_the_first_apply_wrote` applies a
workload (inserts, literals under a term index and a count index, deletes, a delete by pattern, an edge with properties
and its deletion) to one store, and the logical writes each execution leaves (`Executed::logical`) through
`prepare_logical` into a second: the two stores' contents are equal, every key, version and value. It first failed on
one key: a delta's postings in insertion order (the subject-predicates list of `a`), which a logical replay cannot
reproduce; `store_posting`'s `Request::into_writes` now sorts each delta's postings by uid then value (readers sort
deltas themselves), and the check passes. The commit crate's
`test_every_kind_of_round_replays_into_an_empty_store_as_the_first_apply_left_it` does the same end to end over the
WAL: six rounds (inserts and literals, a delete and insert, an edge with properties, a delete by pattern, a session's
request, a predicate's drop) replayed into an empty store give equal contents, and the rebuilt session table answers
the retry. The simulator's changeset workloads (`store_apply` `workload_faults`, `workload_seeds`, `workload_replay`)
pass unchanged.

On the simulator's workload, `ChangesetWorkload::run_replayed(seed, steps)` (`tool_test`, fault-free) keeps a replica:
a second applier over its own simulated store, opened with the same seeds, leases and declarations. Each changeset is
executed on the store and applied there as a round of one at the next version (the stage-5 path); its
`Executed::logical` writes and dictionary entries go through `prepare_logical` and `apply_round_with` into the replica
at the same version; a refused changeset reaches neither; each sync and rollup of a key is performed on both. After
every step the rule `REPLAY_AGREES` compares the two stores' `Contents` (every key with every version, value, meta and
deletion; the keyspace without versions; the applied timestamp and the other metadata) and names the first differing
keys. `store_apply`'s `workload_differential` runs seeds 1–3 for 200 steps each and asserts every write replayed and
more than 100 rounds in all. It fails today, on two defects of the stage, not of the check:

1. **A refused changeset's dictionary entries reach no record.** `ChangesetCore::execute` releases a refused
   execution's terms (`QuadStore::release`), writing their dictionary entries straight to the store; a later round
   naming the term finds it there and carries no entry, so a replica, or a store whose buffered release a lost power
   dropped, cannot resolve it. Seed 1: step 7's `SetProperty` on `<s5> <knows> <s5>` is refused ("no statement is
   held") and leaves the entries of `http://ex.org/knows` in the store alone (`REPLAY_AGREES`, keys without versions);
   past that rule, step 10's insert of a `knows` edge fails replay with "Corrupted dictionary: a system ID has no
   dictionary entry".
2. **The first apply writes a changeset's intermediate states, the logical record only its net effect.** A value
   inserted and replaced or removed within one changeset leaves postings no record names: seed 3's round at version
   52, `[Insert <s1> <friend> <s2>, Insert <s2> <name> "red"@en, Insert <s2> <name> "red"]` on the single-valued
   `name` holding `"02"`, writes a `DEL "red"@en` into `<s2> <name>`'s forward delta and the reverse list of
   `"red"@en`, and a re-set of `name` in that object's predicate list, none of which the replica writes, and a
   statistics delta other than the replica's. Likewise a delete by pattern followed by re-inserting a held value
   (seed 2), and an insert and a delete of edges to one object (seed 10: `[Insert <s3> <friend> <s0>, Delete <s2>
   <friend> <s0>]`), whose first differing key is in the count index: the first apply counts `<s0>` up and down in
   changeset order, replay removes before it inserts.

With the released entries forwarded to the replica by hand (an experiment, not kept), replay agreed at the level of
reads on seeds 1–12 × 300 steps (1,612 rounds): the same statements with the same statement IDs, every forward,
reverse, value-index, count and predicate list equal at the watermark, and the statistics and index invariants held on
the replica; byte-identical contents held on seeds 11 and 12 throughout and failed on the other ten, each first at a
round of the second kind. The spec's scenario asks for "exactly the keys and values", so `REPLAY_AGREES` stays
byte-exact.

## Choices to review

1. **Absent terms by a term key.** A term found absent is recorded as `0xFE` and its ID's 16 bytes; each written
   statement adds the term keys of its subject, predicate, object (an ID) and named graph to its record's conflict
   points, so the history needs no new table. A drop's ranges never hold a term key, so an absent term never conflicts
   with a drop (the spec's table says "one holding a key of that term"; a drop cannot make an absent term present).
2. **The write set counts its dictionary entries.** The round carries a transaction's new dictionary entries, so the
   worker and the commit step bound record + entries by `max_write_set`; a long literal no longer counts in the record.
3. **The applied log position** is a versionless system entry (`\xffultragraphdb/applied_position`, 8 bytes LE)
   written with every applied round's system entries by the commit step and by replay, since no store crate records
   one; recovery reads it and compares it with `OrderedLog::checkpoint()`. `Gap` is unreachable through the WAL, which
   returns rounds contiguously from the position asked (kept for the spec's refusal).
4. **No empty rounds.** A round deciding nothing (retries answered from the table, proposals set aside) is not logged.
5. **Close out of order** stops the partition (`Unavailable`, the reason in `CommitStep::fault`); a close at or below
   the last delivered is ignored.
6. **Busy** answers waiting commits once a close request is `close_request_ms` old and every new commit until the close
   arrives; commits held for a session token are not refused (they wait for an epoch, not a full one).
7. **Catalog versions are per tenant**, taken from closes as announced (in effect from the epoch after the close);
   a kind-1 record without an operation is checked, its conflict placed after the read kinds and before a drop.
8. **Cross-partition** in the transaction checks the subject of `Insert`, `InsertEdge`, `Delete` and `DeletePattern`
   by its IRI's bucket against the placement at the read epoch; claims of `InsertIfAbsent`/`UpsertAndLink` use
   `Bucket::of_value` (system-identity's unique-value bucket, written here though task 7.2 lists its scenario).
9. **Session tables** record no owner (the frontend checks it); a full table refuses a new session busy; `bounded`
   never evicts.
10. **Builds**: a build's end is a kind-3 record with `CompleteBuild(record)` for both swap and undo, and replay derives
    nothing from kind-2 and kind-3 records (referrer sets are group 7's, builds group 6's). A drop of the default graph
    has no record operation and is refused as an invariant.
11. **Round bodies** are written in place with the dictionary section; `RoundBounds::max_write_set` fell by 8 bytes.
12. **`ErrorCode::CrossPartition`** (23) added to `protocol_wire` (announced in the coordination file).
13. Old tests retired with their requirements: registrations through the commit step, owner checks and evictions in
    the partition's table, the clock's versions, the catalog range's conflicts (one skip test), and stage 5's replay of
    build ends from the record's catalog section.

## Mutation survivors, for task 12.1

`scripts/mutants.sh <crate> <file> --diff e3371c71`, one run per file; `replay`, `commit_step/mod.rs`,
`session_entry` and `epoch_stamp` took a second, their first finding the baseline red while a replay scenario was
being written. `prepared`, `transaction`, `close_request` and `replay` leave none.

- `commit_step/commit_step_round.rs` (11, one timeout): the frame and round-size sums in `frame_with` (48), `arrive`
  (124), `gather` (360–379) and `write_body` (628; 643 timed out); `deliver`'s `&&` (217); `validate`'s `stale`
  guard (561).
- `write_set_builder/mod.rs` (6): `encode_into`'s `>` (142); `dictionary_len`'s sum (159).
- `sessions/mod.rs` (5): the request comparisons of `check` (98) and `retry` (121); `begin`'s `&&` (130);
  `committed`'s (147) and `purge`'s (157) `!=`.
- `commit_step/mod.rs` (1): `retry`'s `==` (307).
- `session_entry/mod.rs` (9): `decode_result`'s arms (226–229) — the absent, created and counter results and the
  term index bound.
- `epoch_stamp/mod.rs` (1): `next_after`'s `>` (111), the boundary at the last delivered close.

### Fix of defect 1: a released write writes no dictionary entry

`QuadStore::release` writes nothing. A pending entry counts the unsettled writes holding it (its creator, and each that
took it from the pending map through `resolve`, or through `lookup` for a skolem IRI or reifier, which `WriteTerms`
now carries in `written` too); a release takes one holder away and forgets the entry at none, so the ID is burned; a
commit's settle forgets it outright, since the store then holds it. A write dropped without a release keeps its holds,
so what it held stays pending until a commit holding it settles. Two writes of one round carrying one entry write it
once: the round body's dictionary is a map, and the store batch's equal puts are idempotent; nothing needed deduping.
`workload_differential` now passes seeds 1 and 2 (200 steps each) and first fails at seed 3 step 63 (the version-52
round of defect 2), on forward `0x10`, reverse `0x11`, object-predicates `0x13` and statistics `0x18` keys; no
dictionary key and no "no dictionary entry" error.

Mutants (`--diff e3371c71`, one run per file): `quad_store/mod.rs`, `write_terms/mod.rs`, `changeset_applier/mod.rs`
and `changeset_core/mod.rs` leave none.

- `dictionary/mod.rs` (1): `pending_id` → `None` (172), now read only by `free_skolem`'s check that a drawn skolem IRI
  is not pending, which 104 random bits leave unreachable without a forced collision.

### Fix of defect 2: one derivation for the first apply and replay

A changeset's changes resolve its semantics against the write's own forward and reverse lists, which they read, and
maintain no value index, count index or vector record as they go. `ChangesetWrite::finish` (and so `Execution::finish`,
now returning a `Result`) computes the write's net effect on its snapshot, drops every list and statistics delta its
changes wrote while keeping what they read (`Request::discard_lists`, reading the snapshot through the request's new
snapshot read), and derives every key once with `ChangesetWrite::derive`, the derivation `prepare_logical` runs on a
logged record. Count moves are recorded per member and written once, from the count before the write's first move to
the count after its last, so a count moved and moved back writes no key. `workload_differential` passes byte for byte
on seeds 1–3 × 200 (3.5 s), and once on seeds 1–12 × 300 (26.3 s); every `ultragraphdb_store/...` and
`ultragraphdb_tool/...` target passes (34). Mutation testing of both fixes is deferred to task 12.1 (user,
2026-11-03).

## Release on abort (4.6)

A transaction or proposal ending without being applied releases its dictionary holds on the applier; applying it
settles them instead, so nothing is released twice.

- `Transaction` keeps the applier (`Transaction<'a, S, C, E, B>`, bounds on the struct for its `Drop`). Its private
  `end` releases the open execution with the new `ChangesetApplier::release_execution`, unfinished, deriving nothing
  and unable to fail. It runs at `abort`, at a refusal ending the transaction (cross-partition, a refused changeset, a
  failed read), at the lifetime limit (next request or commit), at a read-only commit, and in `Drop` (a worker or host
  dropping it). A read-write commit refused past the read- or write-set limit releases its finished execution with
  `ChangesetApplier::release`; a commit that succeeds hands the holds to the `Prepared`.
- `Prepared::release(self, applier) -> TxnBuffers` releases a proposal and returns its buffers. `Pipeline::release`
  uses it for every proposal in `StepOutput::released`: an abort at its round's decision (conflict, cross-partition,
  operation refused), a refusal on arrival (too large, busy, unavailable, stopped), a retry answered from the session
  table, and the round in flight, the queue and the held commits when a failed sync or refused round stops the step.
  The commit tests' `Driver` releases the same way. `Prepared::drop` and `Prepared::end_build` release when their
  record is refused. `Prepared::adopt` does not release the execution it replaces: the operated one carries its terms
  and their holds.
- Double release is not harmless. A pending entry counts its holders, not their identities, so releasing one write
  twice while another write holds the entry takes away that other write's hold: the entry is forgotten, the next writer
  of the term creates a second ID, and the holder still commits the first. Releasing a write whose entries a commit of
  another holder settled is harmless, because the entry is gone or names another ID and is left alone. The code
  releases exactly once: `Option::take` in `Transaction`, and `Prepared::release` consumes the proposal.
- Gaps: an execution whose `finish` fails at commit keeps its holds, because `finish` consumes it unreleased. So does a
  round the store refuses to apply, which is durable and replayed after a restart. Proposals still queued in a dropped
  `Pipeline` keep theirs too. Each errs only towards keeping.

Tests: txn `transaction_release_tests.rs` (7: abort, refused changeset, limit at a request, limit at commit,
write-set refusal, drop, commit hands holds to the proposal), `prepared_release_tests.rs` (1: a released ID is never
reused, and one taken stays the taker's); commit `pipeline_release_tests.rs` (5: conflict, too large on arrival,
session retry, failed sync, a committed proposal not released); store_apply `execution_tests.rs` (1:
`release_execution`); and the commit BDD scenario "A released entry never reaches the store". Each probe writes the
term and compares the ID it gets: it takes the holder's ID while the hold lasts and gets a fresh one after release.
Mutants deferred to 12.1.

## Review fixes

Five findings of the adversarial review of the replay-defect fixes. Bazel: every `//ultragraphdb_store/...`,
`//ultragraphdb_transactions/...` and `//ultragraphdb_tool/...` target without the integration tag passes (40, with
`workload_differential`); `cargo fmt -p` and `cargo clippy -p <crate> --all-targets` report nothing for
`store_quads`, `store_apply`, `transactions_txn` and `transactions_commit`. Each new test was run against the code with
its fix taken out and failed there. Mutants deferred to 12.1.

1. **Catalog default entries outside the derivation.** `ChangesetWrite::declaration` recorded an undeclared
   predicate's implicit default entry, in the catalog and the batch, whenever a change named it, while replay records
   one only for the predicates of the net statements it derives. Fix: a change reads the default and records nothing;
   only the derivation (`deriving`, set by `derive`) records it, so the first apply and replay write the default entries
   of the same predicates, those of the statements derived. Tests (`changeset_applier_logical_tests.rs`):
   `[Insert <s> <new> "x", Delete <s> <new> "x"]` applied and replayed leaves equal stores and no `ex:new` in either
   catalog, installed or loaded back; a changeset keeping a statement on `ex:kept` and undoing one on `ex:gone` gives
   both stores `ex:kept`'s default entry and neither `ex:gone`'s.
2. **Declarations in data transactions: not fixed, a question.** Refusing a changeset holding `Change::Declare` in
   `Transaction::write` (a `TxnError` mapped, through `ApplierOperator`'s `refusal`, onto `ErrorCode::Refused`, "the
   catalog refused a write") breaks seven tests that start a build or a catalog change through a data transaction,
   because `Prepared::new` is crate-private and a transaction is the only way to a proposal: txn
   `test_a_declaration_indexing_a_predicate_with_data_starts_a_build`; commit
   `test_a_second_build_waits_for_the_first_builds_end`, `test_a_builds_end_that_cannot_be_proposed_stops_the_pipeline`,
   `test_a_builds_swap_takes_its_version_from_the_step_and_a_later_write_commits`; and the BDD steps "a round admits a
   declaration of `ns` that starts an index build of `p`…", "a declaration of `ns` commits before it", "a term index
   on `name` is being built…". With the refusal, the commit step's build path (`starts_build`, set-aside builds, the
   swap's round) has no producer until the metadata group's catalog path (group 6). Which path should build-starting
   proposals take meanwhile? The change was reverted whole; nothing of it remains.
3. **Commit-time operations leaked holds.** `ChangesetCore::operate` resolves the operations' terms on a clone of the
   transaction's, and every path dropping what they resolved kept the holds they took. Fix: the holds taken beyond the
   original's are released once on every such path, the original's left to the proposal: `WriteTerms::written_beyond`
   and `QuadStore::release_beyond` (store_quads); `ChangesetWrite::release(base)`, called by `operate` when an operation
   is refused and by `ChangesetWrite::finish` when the derivation fails; `ChangesetApplier::release_operated` and
   `Prepared::release_operated` for an operated execution not taken; `Prepared::operated` releases when its record
   cannot be written again; and the `Operator` trait gains `discard`, which the commit step calls for every staged
   proposal it does not adopt (a malformed record, no room in its round, too large, no version left in the epoch), with
   `ApplierOperator` releasing through `Prepared::release_operated`. An adopted operated execution carries every hold
   once and is settled or released whole, as before. Tests: store_apply `changeset_core_operate_tests.rs` (a refused
   compare-and-set on `<new>` leaves no hold and the transaction's own; an operated execution not taken releases only
   what its operation resolved); commit `pipeline_release_tests.rs` (a refused compare-and-set on `<only>` through the
   pipeline holds nothing; an operated commit deferred for a full epoch holds nothing, and decided again after the close
   holds `<only>` until its round is applied, the store then holding that ID). Each probe has two writers in a row take
   the term: one pending ID held makes them equal.
4. **A failed `Execution::finish` kept its holds.** Fix: `ChangesetWrite::finish(base)` releases before returning the
   derivation's error (every hold for an execution, `None`; those beyond `base` for operations), so `Execution::finish`
   releases once and its doc says so; `ChangesetCore::execute` and `Transaction::commit` release nothing more (the
   commit's doc no longer names a gap). Tests: store_apply `execution_tests.rs` (`Fault::FailNextRead` before finish:
   the finish fails, `<new>` is released and nothing written); txn `transaction_release_tests.rs` (a commit whose finish
   the store fails releases once: a second transaction holding `<new>` keeps it, and after its abort the ID is gone).
5. **Round size undercounted operations' dictionary entries.** `gather` counted a commit's dictionary entries before its
   operations ran. Fix: the `Operator` trait gains `staged_dictionary` (`Operated::dictionary`), `fits` takes the record
   and the entries the operations leave, and the round adds the entries of the proposal as decided; a write set no
   longer fitting its round waits for the next, and one no fresh round holds, record and entries together past
   `max_write_set`, is refused too large with that sum (entries are padded to 8, so the sum decides it exactly). The
   `CommitStep` doc names the write set. Test: commit `commit_step_round_size_tests.rs`, a round bounded to hold a plain
   commit with a long literal and an operated commit's record but not the entries of the 2,000-byte IRI its counter
   creates: three rounds, none past the bound, all three committed; without the fix two rounds, the second past it.
