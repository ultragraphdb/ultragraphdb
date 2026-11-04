# Group 7: partitions

## 7.1 Ultra Style lift

`ultragraphdb_tool_bench` and `ultragraphdb_tool_test`, lifted in place. `cargo fmt -p` and `cargo clippy -p
<crate> --all-targets -- -Wclippy::all` report nothing for either; both were run scoped to the two crates rather than
through `make format` and `make fix`, whose `--workspace` and `--fix` would have rewritten crates another agent was
editing. No `BUILD.bazel` changed: both libraries glob `src/**/*.rs`.

Bazel before and after, the same counts. `tool_test` has no tests of its own, so the targets that link it were run
too:

| Target | Tests |
|---|---|
| `tool_bench:ultragraphdb_tool_bench_tests` | 68 |
| `tool_bench:ultragraphdb_tool_bench_src_tests` | 5 |
| `store_apply:workload_differential`, `workload_faults`, `workload_replay`, `workload_seeds` | 1 each |
| `store_kv:crash` | 3 |
| `store_kv:batch_gib`, `crash_drop`, `crash_rebuild`, `crash_temporary`, `fjall_workload_checks`, `fjall_workload_crashes` | 1 each |

Neither crate is on the data plane: `tool_bench` is a benchmark harness run by hand, `tool_test` the helpers of the
store crates' test targets.

### `ultragraphdb_tool_bench`

- **Rule 1.** `DirSizes`' `walk` called itself for each subdirectory. It now walks with a stack of the directories
  still to read, each with its depth, and refuses one deeper than `DIR_DEPTH_MAX` (16) with an I/O error; an entry's
  metadata does not follow a symbolic link, so no directory is pushed twice. The bound has no test at it and one past
  it yet: adding one would change the target's count, which a lift holds. The scan found no other self-call. The
  closures handed to the executor are thread bodies, started at once; the others are called in place.
- **Rule 2.** Every loop's end is stated. The two source threads' loops end at the run's deadline or when their
  consumer stops listening; the apply thread's `recv` loop ends when the source closes the channel, and its `try_recv`
  drain, like the pipeline's `take_waiting`, takes a waiting commit each turn from a source holding at most
  `PENDING_PROPOSALS` unapplied; the pipeline's commit loop ends once the source has closed and nothing is in flight;
  the rollup thread's loop is an event loop ending through `stop`; `load`'s `while left > 0` takes at least one commit
  a turn; the argument parser iterates its arguments. **Not lifted:** `prepare`'s wait for a declaration's build,
  `while a.rebuilding()` with a 10 ms sleep, has no deadline. A deadline needs an error for it, and `BenchErrorEnum`
  has no variant that fits (it is not a usage, apply, store, I/O or commit error); adding one is a surface change, left
  for a decision. A comment states when it ends.
- **Rule 3.** Off the data plane; the runs allocate per commit as the workload they drive does.
- **Rule 5.** Debug checks added: `Summary::of`'s percentiles are ordered (p50 ≤ p99 ≤ max); `Pacer::take_due` has
  released at least the commits due; `CommitShape::next_commit` has hubs and relation types to draw from; an apply step
  holds at most the run's batch cap.
- **Rule 7** (no implicit panic, alongside). `Summary::of` read its samples by index: it now reads them with `get` and
  `last`, the rank clamped as before. `apply_batches` indexed its first commit (`commits[0]`): it measures the commit
  before moving it into the batch. `parse_stat_cpu_ticks` sliced after the last `)`: it takes `rsplit_once(')')`, the
  same text.
- **Rule 8.** The two `#[cfg(test)]` modules of `apply_run.rs` and `pipeline_run.rs` each carry a comment saying why
  they are in `src/` (the report's arithmetic is private); no other `cfg`, no macro.
- **Rule 9.** Every `Arc`, `Mutex` and atomic carries its reason: `ReadCounts`' atomics (the apply, source and rollup
  threads read through one cache and store), `TimedStore`'s mutex and atomics, the `Arc<ReadCounts>` the cache, store
  and snapshot share, the applier's `Arc` (the rollup thread rolls lists up beside the apply thread), and the
  `Arc<AtomicBool>` and `Arc<AtomicU64>` between a run's threads. No trait object.
- **Rule 10.** The three `#[expect(clippy::disallowed_methods)]` (the wall clock in `WallClock` and `TimedStore::apply`,
  the source thread in `pipeline_run`) predate the lift and carry their reasons; none was added.
- `src/utils/op_counts.rs` is in no module tree, so nothing compiles it (an experiment of 2026-11-01). It is left as it
  is; removing it is the user's call.

### `ultragraphdb_tool_test`

- **Layout.** The crate held its types beside its helpers in `src/utils/`. Each type now has its own module under
  `src/types/`: `changeset_workload/` (moved with `git mv`; the private `DataIndexes` in `data_indexes.rs`), `mirror/`
  (the fjall store beside a simulation, private to the crate), `run/` and `test_dir/` (its `Drop` in
  `test_dir_drop.rs`). `run_both`, the crash harness and the store helpers stay in `src/utils/`. The exports of
  `lib.rs` are unchanged, so no caller changed.
- **Rule 1.** `compare_metadata` read both stores' timestamps through a closure taking `&dyn Fn`: it now builds the
  two tuples directly. The scan found no self-call (`TestDir::drop` calls `std::mem::drop`).
- **Rule 2.** `run_until` reads the crash writer's output until its line or the end of the output, when the writer
  exits; the test target's timeout bounds a writer that does neither, as the crash writer's own wait was in group 4.
  `kill_after` reads what a reaped writer printed. Every other loop iterates a collection, a range or a count drawn
  below 3.
- **Rule 3.** Off the data plane.
- **Rule 5.** Debug checks added: the stored key's zero run stays below the 126 that ends a run; a `Run` has found at
  most the versionless reads it replayed, and replayed at most the records it replayed.
- **Rule 7** (alongside). The workload's draws indexed slices (`PHRASES[…]`, a four-element array of predicates and
  one of keyspaces, `candidates[…]`, `keys[…]`): each takes the same draw and reads through `match` or `get`, a missing
  candidate or key taking the path an empty one takes.
- **Rule 8.** No `cfg`, no macro.
- **Rule 9.** The `AtomicU64` naming test directories, and the `Arc` around the applier and the replica, carry their
  reasons. No trait object remains.
- **Not changed:** `ChangesetWorkload`'s counts are public fields (`writes`, `replayed`, `refusals`, `checked`,
  `skipped`, `unrecovered`, `compared`), where the repository's convention is private fields behind getters. The
  `store_apply` workload targets read them; changing them changes those callers, outside this task.

## 7.2–7.3 plan and contracts

Three workstreams, split by crate so none edits another's files. The phase-1 surfaces below are committed as
`unimplemented!()` stubs before the work starts, so no stream waits on another's API.

### Stream Q: `ultragraphdb_store_quads`, `ultragraphdb_store_apply`

store_quads (stubs in place):
- `ReferrerMember` (group, epoch added, generation ≥ 1) and `ReferrerSet` (sorted by group, one member per group):
  `members`, `member(group)`, `at(epoch)`, `register(group, named, epoch) -> Registered { generation, changed }`
  — the generation rule of partitions, "An entity's referrer set names where its inbound edges may be" (absent or
  named ≥ current: max(named, highest) + 1, a renewal keeping its epoch; named < current: unchanged, answer current;
  highest = current, no removal this stage) — and `set(group, generation, epoch)` (keep epoch when a member).
  Codec: version byte 1, count u32 BE, members (group u32, epoch u64, generation u64, BE), sorted, strict decode.
  The value of `QuadKey::Referrers { namespace, entity }` in `Keyspace::Referrers`.
- `QuadError::NotOwned { iri, bucket }`: `Dictionary::resolve` of an IRI whose bucket the store does not own
  (`QuadStore::owning`) and that is neither held, pending nor copied (`WriteTerms::copy`) — subject, object, predicate
  or graph alike. A new blank node draws its bucket among the owned buckets (exists).
- `QuadStore` reads at a version: `referrers(ns, entity, version)`, `mark(ns, entity, group, version)` (value: the
  generation, u64 BE, ≥ 1, of `QuadKey::Mark { namespace, entity, group }`), `subjects`, `subject_predicates`,
  `object_predicates`, `reverse_count`, and the read kernel `scan(&KernelScan, &mut Vec<KernelRow>) -> resume`.

store_apply (stubs in place):
- `Execution::subjects_into`, `lookup_into` (value-index read by tokenizer), `adopt(iri, id)` (→ `WriteTerms::copy`),
  `read_mark` (recorded in the read set), `mark(entity, group, generation)` (reads the mark key into the read set,
  writes it at commit, lists it in `Executed::marks`), `unique_writes()` (predicate ID and value encoding of every
  plain write of a unique predicate).
- `objects_into` consults the subject-predicates list first and reads no list it does not name (partitions, "Absence
  is proven by a node's predicate lists"); a node created in the execution reads no list.
- The home's own groups: an execution inserting an edge from a subject of group `g` to an IRI or blank-node object
  `o` whose bucket the store owns adds `g` to `R(o)` at generation 1 when `R(o)` lacks it, derived when the round
  applies, with the commit version's epoch, add-if-absent, so concurrent commits never conflict on `R(o)`; replay
  derives the same.
- `ChangesetApplier::register(ns, EntityRef) -> Registering` (gets or creates the ID, holding its entries) and
  `Registering::finish(group, generation) -> Executed` (writes the member through `ReferrerSet::set` at apply time,
  in round order, plus the IRI's dictionary entries); `prepare_replayed_registration(ns, entity, group, generation)`.
- `RecordWrites` gains the record's section 4 (`with_marks(Vec<MarkedReference>)` or equivalent); `prepare_replay`
  writes each mark.

### Stream T: `ultragraphdb_transactions_txn`, `ultragraphdb_transactions_commit`

- `Transaction` (stubs in place): `subjects_into`, `lookup_into` (allowed to a bound transaction only while its
  partition owns the namespace's whole run at the read epoch, else `CrossPartition` naming the lowest other partition
  of the run), `adopt(iri, id, close)`, `read_mark`, `mark(entity, group, generation, close)`, `epoch_floor()`
  (max close + 1 of every reply). `objects_into` refuses a subject another partition owns. A write's subject that is
  an existing blank node is checked by its skolem IRI's bucket. After execution, every `unique_writes()` value whose
  bucket (`Bucket::of_value`) another partition owns aborts `CrossPartition`. `WriteSetBuilder` writes
  `Executed::marks()` into section 4.
- `Prepared::epoch_floor()`; `Proposal::epoch_floor()` (default `None`); the commit step holds a proposal until
  `max(token epoch, epoch floor)` is open, as it holds a token now.
- `Pipeline::registration(ticket, executed: Executed, record, now_us)`: the round applies the registration's
  `Executed` at the registration's own commit version; `Pipeline::delivered() -> Epoch`, the latest close delivered.
  `Pipeline` can bind its `ApplierOperator` (claims at commit) to a partition and placement.
- Replay: a kind-2 record → `prepare_replayed_registration`; a transaction record's section 4 → the marks.

### Stream P (me): `ultragraphdb_cluster_partition` (new), `ultragraphdb_core_ids` BDD

The partition assembly (log, commit step, store over owned buckets, sequence blocks, epoch delivery), routing with
the owner named, the home's registration service (generation rule against `R(o)` plus an overlay of its undurable
decisions, one kind-2 record per registration, reply once durable with ID, generation and delivered close), the
referrer side (foreign references of a changeset, marks consulted, registrations batched per home per request,
replies adopted and marked), Drop across partitions refused, reverse reads at a closed epoch through `R(o)` and the
placement map, summed reverse counts; the scenarios of 7.2 that need several partitions.

### Scenario homes (7.2)

| Scenarios | Crate |
|---|---|
| index-maintenance "Count indexes" (single-partition ones), partitions "Absence is proven…" | store_apply (Q) |
| partitions "A read-write transaction stays within one partition…" (single-store ones), "A delete by pattern…", write-ahead-log "Replay derives the same keys", "Replay derives a home's referrer-set deltas" | transactions_commit (T) |
| system-identity "A unique value's bucket is computed from its predicate and value" | core_ids (P) |
| partitions (multi-partition), catalog "Unique values" two-partition claim, term-dictionary registration, quad-reads reverse reads, "A reverse count sums the referrers' partitions" | cluster_partition (P) |

## Stream T (transactions layer)

`ultragraphdb_transactions_txn` and `ultragraphdb_transactions_commit`, against Stream Q's phase-1 stubs.

### Phase 2: the failing run

Bazel, the suite written against the stubs (`Transaction`'s partition bodies, `Prepared::epoch_floor`,
`Pipeline::registration`, `delivered` and `bound` all `unimplemented!()`), before any body was written:

| Target | Run | Failing for |
|---|---|---|
| `transactions_txn:ultragraphdb_transactions_txn_tests` | 116 passed, 17 failed of 133 | 14 the stubs' `unimplemented!()`; 3 the behaviour missing (a forward read of a foreign subject taken, a foreign unique value taken, an existing blank node checked by the bucket its text derives) |
| `transactions_commit:ultragraphdb_transactions_commit_tests` | 234 passed, 13 failed of 247 | 6 the stubs (pipeline, transaction); 4 Q's `register`; 3 the step's behaviour missing (a floor ignored, twice; a registration's dictionary entries not logged); the held-commit expiry test was then made to discriminate (a token-held commit beside the floor-held one) |
| `transactions_commit:bdd` | 80 scenarios, 71 passed, 9 failed | the 11 new scenarios: 7 `unimplemented!()`, 2 assertions; "A transaction reaching a second partition aborts" and "A delete by pattern of another partition's subject is refused" passed, behaviour that predates the stage (the subject check of `write`) |

### Corner cases and the tests covering them

`txn` = `ultragraphdb_transactions_txn/tests/types/transaction/transaction_partition_tests.rs`, `step` =
`commit/tests/types/commit_step/commit_step_epoch_tests.rs`, `pipe` = `commit/tests/types/pipeline/pipeline_tests.rs`,
`replay` = `commit/tests/types/replay/replay_tests.rs`, `bdd` = `commit/features/partitions/partitions.feature` and
`commit/features/write-ahead-log/replay_derives.feature` (steps in `commit/tests/partition_steps.rs`).

| Case | Test |
|---|---|
| Reverse read, unbound: the subjects, own writes on top, `out` cleared first | txn `test_an_unbound_transaction_reads_the_subjects_pointing_at_an_object` |
| Reverse read recorded in the read set | txn `test_a_reverse_read_is_recorded_in_the_read_set` |
| Bound, one partition owns the whole run: reads, then writes and commits | txn `…owning_the_whole_run…`; bdd "One partition reads its own reverse entries" |
| Bound, run spread: `CrossPartition` naming the lowest other partition, for 2 and 3 partitions, bound to the first, middle and last | txn `test_a_reverse_read_over_a_spread_run_aborts_naming_the_lowest_other_partition`; bdd "A reverse read over a spread namespace…" |
| A cross-partition read ends the transaction: every later request `Ended`, nothing committed | txn `…spread_run_aborts_and_writes_nothing` |
| A first lookup over a spread run aborts and holds nothing | txn `test_a_first_lookup_over_a_spread_run_aborts_and_holds_nothing`; bdd "A first read naming no subject…" |
| A lookup by a declared `term` index finds its subjects | txn `test_a_lookup_finds_the_subjects_whose_value_matches_by_its_index` |
| A reverse read past the limit: `PastLimit { 10000 }`, ended | txn `test_a_reverse_read_past_the_limit_ends_the_transaction` |
| A cross-partition refusal releases the dictionary entries the transaction created | txn `test_a_cross_partition_read_releases_what_the_transaction_created` |
| Forward read of another partition's subject after an own one (write skew) | txn `…forward_read_of_another_partitions_subject…`; bdd "Write skew across partitions…" |
| Unbound: every subject read | txn `test_an_unbound_transaction_reads_every_subject` |
| An existing blank node by its skolem IRI's stated bucket, written and read; with two partitions one of the two bindings disagrees with the text's derivation whichever the IRI | txn `test_an_existing_blank_node_is_checked_by_its_skolem_iris_bucket` |
| A plain write of a unique value another partition owns aborts, nothing held | txn `…unique_value_another_partition_owns_aborts`; bdd "A plain write of a unique value…" |
| A unique value the partition owns is taken; a non-unique predicate's value claims nothing | txn `…unique_value_the_partition_owns_is_taken` |
| Insert-if-absent of a value another partition owns, through a bound pipeline | bdd "A unique value owned by another partition is refused"; pipe `test_a_bound_pipeline_claims_only_its_partitions_values` |
| Write to a second partition after a first; delete by pattern of a foreign subject; delete by pattern at the home leaves inbound edges | bdd, the three scenarios |
| No reply: no floor, on the transaction and its proposal | txn `test_a_transaction_without_a_reply_has_no_epoch_floor` |
| Floor = max(close + 1) over replies: an earlier close lowers nothing, a later raises | txn `test_the_epoch_floor_is_one_above_the_latest_close_a_reply_named` |
| An adopted IRI resolves to the home's ID | txn `test_an_adopted_iri_resolves_to_its_homes_id` |
| A mark: absent before, seen after, listed in section 4, its key in the read set; none without marks | txn `test_a_mark_is_read_written_and_listed_in_section_4`, `…without_marks_lists_no_registration` |
| `adopt`, `mark`, `read_mark` past the limit and once ended; a refused reply raises no floor | txn `test_replies_to_an_ended_or_expired_transaction_are_refused` |
| A renewal between the reply and the commit: `Conflict { List }`, nothing written, the retry marks the new generation | pipe `test_a_renewal_between_the_reply_and_the_commit_conflicts` |
| Floor above open: held through closes until open (close 11 → epoch 12) | step `test_a_commit_with_an_epoch_floor_waits_for_the_close_before_it` |
| Floor at or below open holds nothing | step `test_a_floor_at_or_below_the_open_epoch_holds_nothing` |
| The larger of token and floor holds, each way round | step `test_the_larger_of_the_token_and_the_floor_holds_the_commit` |
| Held for a floor or a token: refused busy once the close request is unanswered for `close_request_ms` | step `test_commits_held_for_a_floor_or_a_token_are_refused_busy…` |
| A registration's dictionary entries in its round's dictionary section | step `test_a_registrations_dictionary_entries_are_in_its_rounds_dictionary_section` |
| The step names the registrations applied (ticket, version) and dismissed: malformed, in flight at a failed sync, waiting, arriving stopped | step `test_the_step_names_the_registrations_it_applies_and_those_it_dismisses` |
| A registration through the pipeline: answered with its version | pipe `test_a_registration_through_the_pipeline_is_answered_with_its_version` |
| Its member at its own version and not below | pipe `test_a_registration_applies_its_member_at_its_own_commit_version` |
| A commit and a registration in one round, each at its version, in round order | pipe `test_a_round_of_a_commit_and_a_registration_applies_both_at_their_versions` |
| A refused registration releases what its home held; the refusal names the wire's reason | pipe `test_a_refused_registration_releases_what_its_home_held` |
| `delivered()` before and after a close | pipe `test_the_pipeline_names_the_latest_close_delivered` |
| Replay of two registrations: each member at its listed version, the store as the first apply left it | replay `test_a_round_of_two_registrations_replays_each_member_at_its_version`; bdd "Replay derives a home's referrer-set deltas" |
| Replay of a reference: the mark and the copied entry, the store as the first apply left it | replay `test_a_reference_with_its_mark_and_copied_entry_replays_the_same_keys`; bdd "Replay derives the same keys" |

### Deviations from the contract

- **A registration's dictionary entries are logged.** `Input::Registration` gains `dictionary`, the entries the home
  created for the entity, which its round's dictionary section holds; `Pipeline::registration` takes them from the
  `Executed`. Without them a home's new ID would be applied and not replayed (write-ahead-log, "A round's record is
  logical": "the new dictionary entries the round creates").
- **`StepOutput::registered()` and `dismissed()`.** The step is generic over its proposals and carries no `Executed` for
  a registration, so the pipeline keeps each one by ticket and the step names, in round order, the registrations applied
  at their versions and those it will not apply; the pipeline applies or releases them.
- **Held commits expire.** A commit held for its token or its floor is refused busy, beside the commits waiting for a
  full epoch, once the close request goes unanswered for `close_request_ms`. Before, a token-held commit was never
  refused while the step lived.
- **Unique values are checked only when bound**, as subjects are: unbound, every value is taken.
- **A floor past the last epoch** saturates at `Epoch::MAX`, where the step stops before stamping.
- **A blank node by its label** in `objects_into` is the transaction's own, as in a write.

### Phase 4: the run once Q landed

| Target | Run |
|---|---|
| `transactions_txn:ultragraphdb_transactions_txn_tests` | 133 passed |
| `transactions_txn:bdd` | 6 scenarios passed |
| `transactions_commit:ultragraphdb_transactions_commit_tests` | 247 passed, 1 failed: the section-4 replay test below |
| `transactions_commit:bdd` | 80 scenarios, 79 passed, 1 failed: "Replay derives the same keys" (the mark is not replayed; the copied entry and the first apply's mark are) |
| `transactions_log`, `//ultragraphdb_tool/...` | unchanged, passed |

Three tests that predate the stage counted the lists a forward read records: Q's `objects_into` now reads the
subject's predicate list first (partitions, "Absence is proven by a node's predicate lists"), so
`test_a_read_write_transaction_with_no_writes_is_still_validated` and `test_a_prepared_transaction_proposes_its_read_set_and_record`
expect 2 lists and `test_a_read_set_past_its_limit_refuses_the_commit` 114 bytes (the 41-byte subject-predicates key and
its span, 49, beside the forward list's 65), derived from the key layout in its comment.

### Coordinator additions (2026-11-04)

- **A registration's dictionary entries are durable in its round.** `Pipeline::registration` takes the dictionary
  keyspace's entries of the registration's `Executed` (from `Registering::finish`, or `Registering::into_executed` for
  one with no member) into `Input::Registration::dictionary`; the round admitting it writes them into its dictionary
  section beside its transactions' `Prepared::dictionary()` entries; the round's apply settles them, and a registration
  the step dismisses is released on the applier (`ChangesetApplier::release`). Covered by step
  `test_a_registrations_dictionary_entries_are_in_its_rounds_dictionary_section`, pipe
  `test_a_refused_registration_releases_what_its_home_held` and replay
  `test_a_round_of_two_registrations_replays_each_member_at_its_version` (store contents equal).
- **A kind-2 record holds one registration or none.** `admit_registration` checks only the record's kind, so both
  shapes are admitted with a commit version of their own; replay derives a kind-2 record through
  `prepare_replayed_registration` only when its section 4 holds a registration, and nothing beyond the round's
  dictionary section otherwise. No test of the empty shape yet: it waits on the wire's constructor for an empty kind 2,
  the coordinator's change.

### Open: replaying a transaction record's section 4

Replay derives a transaction record through `ChangesetApplier::prepare_logical(namespace, &LogicalWrites, dictionary)`,
not `prepare_replay(RecordWrites)`, and `apply_round_with` refuses two executions at one version, so the marks cannot
be a second execution beside the logical one. Q's `RecordWrites::with_marks` does not reach this path. It needs the
marks on the logical path (`LogicalWrites` or `prepare_logical`); left for Q or the coordinator. Waiting on it: replay
`test_a_reference_with_its_mark_and_copied_entry_replays_the_same_keys` and bdd "Replay derives the same keys".

## Stream Q (store layer)

### Phase 2: the failing run

Against the phase-1 stubs, under Bazel: `ultragraphdb_store_quads_tests` 37 failed of 140 (103 passing: the existing
suite and the new tests of plain getters, the `NotOwned` error, held and copied IRIs); `ultragraphdb_store_apply_tests`
28 failed of 272; store_apply `bdd` 1 scenario failed of 90 ("A predicate the node lacks is not read"). The three
"Count indexes" scenarios rebound to the spec's wording pass against the existing maintenance, as expected: their
behaviour predates the group. Final run: quads 142/142, quads bdd 22/22, apply 276/276, apply bdd 90/90, every target of
`//ultragraphdb_store/...`, `//ultragraphdb_transactions/...` and `//ultragraphdb_tool/...` green (40 tests).

### Corner cases and the tests covering them

store_quads (`tests/types/`):
- Referrer set rule (`referrer_set/referrer_set_tests.rs`): empty set; first registration (group 7, epoch 12, generation
  1, unseen at 11); repeat naming an older generation (unchanged, answers 3, epoch kept); renewal (3 → 4, epoch kept);
  named generation above current (9 → 10); new group naming 5 (→ 6); `u64::MAX` saturates; sorted one per group with a
  repeat; groups 0 and 2^18 − 1; `at(epoch)` over three epochs; `set` inserting, lowering and keeping the epoch.
- Codec (`referrer_set/referrer_set_codec_tests.rs`, bytes written by hand): empty set; two members; one member at the
  largest group, epoch and generation; encode appends; version byte 0, 2 and an empty value; truncated header, missing
  member, cut member; trailing byte; unsorted; duplicate group; zero generation; group 2^18; epoch 2^42; a count of
  2^32 − 1 refused before allocating.
- `QuadError::NotOwned` (`dictionary_tests.rs`, `errors/quad_error/quad_error_tests.rs`): refused as subject, object,
  predicate, graph and bare IRI, naming the IRI and its bucket; an owned IRI beside it; a held IRI resolves; a copied IRI
  resolves to its home's ID and its entries are written; blank nodes draw owned buckets; a store alone owns all.
- Partition reads (`quad_store_partition_tests.rs`): referrer set absent, at two versions, in another namespace, corrupt;
  mark absent vs present at versions, another group's, zero and 7-byte values; subjects and reverse counts at two
  versions, of an object nobody points at, of a literal; a reverse list the object-predicates list does not name is not
  read (OP entry removed, reverse list kept); predicate lists at versions and for nodes without lists; the kernel in key
  order, across namespaces, before the write; limit 1 and resume; a limit filled exactly; an exclusive end; a scan past
  the namespace's end; a list emptied by a delete gives no row; non-list keyspaces refused.
- `QuadWrite::names_predicate`, `points_with`, `ids` (`quad_write_tests.rs`), `KernelScan`, `KernelRow`,
  `ReferrerMember` getters.

store_apply (`tests/types/`):
- `execution/execution_partition_tests.rs`: subjects behind the OP list with own writes; a reverse read of an object
  with no object-predicates entry reads no reverse list; an object created in the execution reads no list; literal,
  unknown literal and unnamed blank node; lookup with own writes, ID order and the index key read; `NoIndex` for an
  undeclared tokenizer, an undeclared predicate and an unknown IRI; `objects_into` reads the SP list and no `age` list;
  own writes of a held node; a node created reads no list; `adopt` (refused `NotOwned` without it, entries copied both
  ways, ID held after apply); mark absent/present/own, marked twice keeps one at the later generation, read set holds the
  key, written at commit; a renewal's commit writes the key a reply's commit read; `unique_writes`; a read of a foreign
  predicate refused `NotOwned`.
- `changeset_applier/changeset_applier_partition_tests.rs`: registration of a new IRI (ID derived, entries written by its
  round, member at epoch 12 versioned); by ID and repeat by IRI give one ID; unknown ID is corruption; a foreign IRI is
  `NotOwned`; a released registration burns its ID; two registrations of one entity in one round both land at their
  epochs; renewal keeps the epoch; the home's own group at epoch 8, never in the read set; a later edge keeps it;
  concurrent first references add it once; a foreign object joins no set here; replay (logical) derives the own groups
  alike; replayed registrations at their versions ("Replay derives a home's referrer-set deltas").
- `record_writes_tests.rs`: no section 4 writes no mark; `RecordWrites::with_marks` and `LogicalWrites::with_marks`
  replayed write each mark at the record's version. `changeset_core_operate_tests.rs`: an operated transaction keeps its
  marks and its edges join referrer sets. `changeset_core_drop_tests.rs`: data, namespace and full drops remove marks and
  referrer sets; a graph drop keeps them. `errors/`: `NoIndex` constructor and text.
- BDD: index-maintenance "Adding a third object moves the count", "Re-writing an existing object leaves the count",
  "Reverse counts are kept" in the spec's wording; `features/partitions/partitions.feature` "A predicate the node lacks
  is not read".

### Design as built

- The home's own groups are derived from the writes, not from the changes: `utils/referrer_candidates.rs` reads each
  forward delta's set references to IRIs and blank nodes of owned buckets, filters out `(o, g)` the set names at the read
  version (skipped for objects the write created), and `ChangesetCore::derive_referrers` re-checks them in `write_round`
  under the apply lock, add-if-absent at generation 1 with the commit version's epoch, beside the registrations' `set`, in
  round order. Execution, `prepare_logical`, `prepare_replay` and `operate` all derive candidates the same way, so every
  replay path writes the same referrer sets. The set is no read of the execution, so concurrent commits never conflict
  on it.
- Mark keys read are recorded among the read set's whole-list keys (`Reads::lists`), so a commit writing the key
  conflicts as a list read does.
- `lookup_into` intersects the token lists; since a running execution keeps no index until it finishes, a subject whose
  list of the predicate it changed is judged by that list as it holds now.

### Contract deviations

- `ultragraphdb_store_posting::Writes::add_entry` (new, 4 lines): the referrer sets are written when the round applies,
  into an execution's writes, with no copy of its deltas; `Writes` had no mutator.
- `ChangesetErrorEnum::NoIndex { predicate, tokenizer }` (new), the refusal `lookup_into` names; its arm added to the
  exhaustive match in `ultragraphdb_tool_test` (`changeset_workload/mod.rs`).
- `QuadStore::owned()` (new accessor), and `QuadWrite::names_predicate`, `points_with`, `ids` (new reads).
- `RecordWrites::with_marks` added, `RecordWrites::new` unchanged, so no caller changed for it. At the coordinator's
  request `LogicalWrites::with_marks`/`marks()` carry section 4 on the path replay uses; `prepare_logical` writes each
  mark; `ultragraphdb_transactions_commit/src/types/replay/mod.rs` passes the record's registrations (the one authorized
  edit).
- `QuadStore::scan`'s resume key is the first key of the range not read, to pass as the next scan's `from` (the stub's
  "the key past which" was ambiguous).
- Data, namespace and full drops now also drop the marks and referrers keyspaces (a graph drop does not: they name no
  graph).
- Registrations execute under an empty catalog, which leaves the installed one alone, instead of cloning it.

### Open

- A pending (unsettled) IRI of a bucket the store does not own cannot arise, so it is not tested.
- Mutation testing, fuzzing of `ReferrerSet::decode` and coverage are task 12.1's.

## Stream P (partition crate)

`ultragraphdb_cluster_partition`: `Partition` (routing, `owned`, `owns_run`, a bound `begin`, `check_drop`, and as a
referrer `references`, `read_references`, `operation_references`, `adopt`), `Home` (the entity's home: an overlay of
decisions not yet applied over the stored referrer sets, one kind-2 entry per decision, answers once durable), `Fanout`
(reverse reads, reverse counts, the lookup by summed reverse count, at a closed epoch), `Reference`, `Answer`,
`ReverseQuery`, `PartitionError`. `core_ids` gains the golden `unique_values.json` from `unique_values.py`, the
xxHash reference over the fields the rule names.

### Phase 2: the failing run

Against the stubs, `:ultragraphdb_cluster_partition_tests` 5 passed and 19 failed (the 5 are the error type and the
plain value types), `:bdd` 28 of 28 scenarios failed, each on an `unimplemented!()` in the crate. The final suite adds
8 unit tests the implementation asked for: `rdf:type`, reads, operations, a new blank subject, and what the BDD target
alone exercised before.

### Decisions made building it

- **A kind-2 record may hold no registration** (`PROTOCOL.md` §12.2, `WriteSetRecord::entity`): a predicate's or
  graph's ID a home gets or creates joins no referrer set and still must be durable in a round before the reply.
- **A home answers from the entry's commit version**: the close is the epoch the entry was stamped at, less one, so
  an entry deferred past a close answers the close it really stamped above. A repeat with nothing in flight answers the
  latest delivered close.
- **A repeat of a member still in flight waits for the entry that decided it** (a waiter on that entry's ticket), so no
  reply names a generation that is not yet durable.
- **`rdf:type` is got at its home before a namespace's first write**: the first write installs the catalog, which names
  it. `references` always asks for it when it is unheld and homed elsewhere.
- **Reads get or create, so a read is a first use**: the store resolves what a read names so a term no statement holds
  is recorded absent by its ID. `read_references` gets a foreign predicate's or graph's ID before the read; without it
  the read is refused `NotOwned` rather than silently unrecorded, which would lose a conflict.
- **A claim naming a foreign node is refused `CrossPartition`** (`operation_references`): the node an insert-if-absent
  or upsert-and-link makes takes its bucket at the commit step, after registrations are done, so its group cannot
  register, and a value or link naming a node homed elsewhere would be an inbound edge no referrer set names.
- **A new blank subject's group comes from its ID** (`Transaction::node_id`), resolved on the transaction that writes
  it, so the group registered is the one the write uses.
- **A closed read reads at the lower of the epoch's top and the partition's watermark**: a read above the watermark
  waits for it, and a partition that has closed the epoch has applied everything at or below it.

### Scenario numbers

`A write reaches its bucket's partition` and `A misrouted write is refused with its owner` check bucket 30,000 itself
through routing under a map placing it in partition 2; namespace 0's subjects lie in that namespace's run, so a subject
partition 2 owns stands for the write. A scenario naming a bucket group a subject must come from (3, 7, 9) uses that
subject's own group, as comments at the steps say; a registration's group, an input, is the number itself.

### Left to the roles (group 9), by design

- **Binding a transaction's first operation** is the frontend's routing: `route` refuses a wrong partition naming the
  owner; the frontend chooses by `iri_bucket` or `Bucket::of_value`.
- **Calling the checks**: the storage host calls `check_drop` before a Drop, `read_references` before a read, and runs
  the referrer flow on a write. `references` and `adopt` take the same transaction, so the blank-node IDs agree.
- **Placement changes are not applied live**: `Partition::owned` is computed when the store opens and the partition's
  map is its copy; moves and splits are stage 8's.

### Not covered yet

A home's `settle` with a refused entry and a pipeline that stops mid-registration have no test (task 7.4's fault runs
reach them); fuzzing `ReferrerSet::decode` and coverage wait for task 12.1 with the other deferred hardening.
