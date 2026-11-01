# Group 6: transactions on a worker

`ultragraphdb_transactions_txn`, with an execution held open across requests added to `ultragraphdb_store_apply` and a
zero-copy encoder added to `ultragraphdb_core_codec`. Built by an agent until the API failed it during the audit (HTTP
529) and the machine restarted; the orchestrator finished the audit, the gaps, the bench's wiring and these notes.

## 6.1 Scenarios

`features/transactions/transactions.feature` holds the six scenarios of the four requirements verbatim: "A read above the
watermark is refused", "A transaction sees every commit acknowledged before it began", "A read-only transaction does not
abort", "A read after a write sees it", "A read-write transaction past its limit aborts", "An aborted transaction writes
nothing". Run by the crate's cucumber target `bdd`.

## 6.2 Surface

| Item | What it is |
|---|---|
| `Transaction<'a, S, C>` | A transaction on a worker: `begin` at the watermark W (read version 0) or at a named version at or below W, refusing one above W at once with `ReadAboveWatermark` naming both; `write` executes a changeset on top of its earlier writes; `objects_into` reads its snapshot with its own writes on top, recorded; `commit` ends it; `abort` writes nothing and gives its buffers back |
| `Mode` | `ReadWrite` or `ReadOnly` |
| `Commit` | `ReadOnly(TxnBuffers)`, ended on the worker and never validated, or `ReadWrite(Prepared)`, for the commit step |
| `Prepared` | The executed writes, the sealed read set and the write-set record, with `into_parts` to recover the execution and the buffers |
| `ReadSet` | Lists read whole, statements tested, key ranges scanned, dictionary keys found absent, and the namespace's catalog range, always present |
| `WriteSetBuilder` | Writes an `Executed` straight into a buffer in `protocol_wire`'s `WriteSetRecord` layout |
| `TxnBuffers` | A read set and a record buffer, reused from transaction to transaction |
| `TxnLimits` | The settings below |
| `TxnError(pub TxnErrorEnum)` | `ReadAboveWatermark`, `PastLimit` (the only retryable one), `ReadOnly`, `Namespace`, `Ended`, `ReadSetTooLarge`, `WriteSetTooLarge`, `Changeset` |

| Setting | Default |
|---|---|
| `read_write` | 10 s |
| `read_only` | 16 s |
| `max_write_set` | 1 MiB (`MAX_FRAME`) |
| `max_read_set` | 1 MiB |
| `objects_per_list` | 64 |

A byte bound above `u32::MAX` is taken as `u32::MAX`, because a read set's and a record's offsets are u32. Time is the
caller's: every request carries the worker's monotonic clock in microseconds, so the simulator drives the limits
without waiting. A transaction past its limit, or whose changeset or read is refused, is ended: every later request is
refused with `Ended`, and it has written nothing.

**Outside the crate.**
- `ultragraphdb_store_apply`: `Execution`, an execution held open across a transaction's requests at its read version
  (`ChangesetApplier::begin`, `ChangesetCore::begin_execution`), its `apply`, `objects_into` and `finish`;
  `ChangesetCore::execute` is now `begin_execution` + `apply` + `finish`. `ChangesetWrite::objects_into` reads a
  subject's predicate as the write sees it and records the read, or the subject's absence when the subject is unknown.
- `ultragraphdb_core_codec`: `PostingList::encode_into`, appending the posting format to a caller's buffer, which the
  write-set builder uses so a delta is encoded once, into the record.

## Performance by design

Decided by the user on 2026-10-27 for groups 6 to 8 (tasks.md's header): the low-hanging memory and micro-optimizations
go in before any benchmark.

| Choice | Why | Tested by |
|---|---|---|
| The read set's keys in one byte arena, named by u32 spans | one allocation per buffer instead of one per key | the set-model property test over random pushes; a span one byte off is planted (R07) |
| Lists, statements, ranges and absences as vectors, sorted and deduplicated once at `seal` | the commit step merge-joins them against its history; no tree per transaction | `test_the_sealed_read_set_agrees_with_a_set_model_over_random_pushes`; dedup and sort plants (R04, R08, R13, R15) |
| `TxnBuffers` reused across transactions; `reset` clears, never reallocates | no allocation per transaction in the steady state | `test_a_reused_read_set_keeps_its_capacity_and_grows_no_more_for_what_fits`, `test_the_buffers_are_reused_from_transaction_to_transaction`, `test_an_abort_gives_back_the_buffers_it_was_given` |
| The record's size computed first, reserved once (`reserve_exact`), written in one pass | no growth and no intermediate copy of the writes | `test_the_written_size_reserves_once`; doubled reserve planted (W13) |
| Deltas encoded straight into the record (`encode_into`) | the posting list is encoded once | the record's round trip through `WriteSetRecordView` |
| Bytes accounted per transaction: a key's bytes plus 8 per span; refusal past the limit leaves the set as it was | memory bounded with a typed refusal, never unbounded growth | `test_a_push_at_the_limit_is_taken_and_one_past_it_is_refused_adding_nothing`, `test_a_gather_past_the_limit_leaves_the_read_set_as_it_was`, `test_a_limit_past_u32_is_capped_where_spans_reach` |
| No error built or formatted on the success path | errors cost only when returned | review |

**The bench** (`benches/read_set_bench.rs`; `cargo bench -p ultragraphdb_transactions_txn --bench read_set_bench`,
`bazel run //ultragraphdb_transactions/ultragraphdb_transactions_txn:read_set_bench -- --bench`; never in `make test`).
A transaction's forward-list keys of 25 bytes, one in four repeated, gathered and sealed. Measured 2026-10-28 on the
development Mac (Apple silicon), 1 s warm-up, 3 s measurement, medians:

| Keys | Arena, reused | A `Vec<u8>` per key, fresh vector | Fresh `BTreeSet<Vec<u8>>` |
|---|---|---|---|
| 64 | 1.54 µs | 2.37 µs | 3.19 µs |
| 1,024 | 34.5 µs | 46.8 µs | 51.7 µs |

The arena is 35% faster than a vector of owned keys at 64 keys and 26% at 1,024, and allocates nothing once its
buffers are warm. It measures the read set alone, not the execution that records the keys or the resolver that walks
them.

## Phase 2: the failing run

The agent kept its phase-2 record in the session's scratchpad, which the machine's restart cleared before it was copied
here, so the failing count is lost and cannot be shown from what remains. The audit below is the evidence that the
suite rejects defects in the implementation.

## Phase 3: the audit

52 defects, each planted alone in the tree and restored in a `finally`, the owning suites run under Cargo (the txn
crate's, and `store_apply`'s for E01 to E04); files hashed before and checked after. R01 to R09 ran under the agent and
were each rejected by a test aimed at the rule. The machine restarted during E03 with it planted; it was found by
checking every defect's original text in the tree and restored. R10 to E04 were then run again.

| Result | Defects |
|---|---|
| Rejected at once by a test aimed at the rule | 44: R01–R10, R12, R13, R15, W01–W11, W13–W15, T01–T12, T15, T17, E01–E03 |
| Survived; a test added, the defect planted again and rejected | 7 (below) |
| A plant that did not compile, replaced and rejected | 1: T13, the limit shown in milliseconds always |

| Survivor | Why it survived | Closed by |
|---|---|---|
| R11, a gather dropping the ranges | the gather test's execution scanned no range, so both sides were empty | `test_gathering_a_declarations_scan_records_its_ranges` (narrowing a predicate with data) |
| R14, a limit past `u32::MAX` not capped | no test set one | `test_a_limit_past_u32_is_capped_where_spans_reach` |
| W12, a removal written as an empty value | no test deleted a record | `test_a_deleted_edges_statement_record_is_written_as_a_removal` |
| T14, an abort giving fresh buffers back | only the commit's reuse was tested | `test_an_abort_gives_back_the_buffers_it_was_given` |
| T16, the limits' objects bound not applied | the default bound and the builder's own default agree | `test_the_objects_bound_of_the_limits_names_a_list_whole` (a bound of 0) |
| T18, a failed read leaving the transaction open | no test failed a read | `test_a_read_the_store_fails_ends_the_transaction` (`Fault::FailNextRead`) |
| E04, the predicate looked up before the subject | every test knew one of the two | `test_a_read_of_an_unknown_subject_and_predicate_records_the_subject_absent` |

## Phase 5

The crate decodes no bytes from a client, a peer or the disk beyond what `protocol_wire` already fuzzes, so it has no
fuzz target. Mutation testing runs at the stage's close (11.8).

## Decisions taken here, for review

- **Dependencies beyond design.md's table.** The table names `_wire` and `ultragraphdb_store_apply`; the crate also
  depends on `ultragraphdb_core_codec` (the posting format the record holds), `ultragraphdb_core_keys` (the keyspace a
  key belongs to), `ultragraphdb_core_mapping` (`Changeset`), `ultragraphdb_store_posting` and
  `ultragraphdb_store_quads` (the cached list type and the dictionary tags), `ultragraphdb_traits_store` (`Store`,
  `ListCache`) and `oxrdf` (the read's terms). All sit below it.
- **A statement's absence is recorded at the first unknown term.** A read of an unknown subject records the subject
  absent and stops: a later creation of the subject is what would change the read, so that absence is its conflict.
- **Only a forward list is named object by object** in the record's conflict statements, up to `objects_per_list`
  objects; any other list, a longer one, or one holding a posting that names no object is a conflict point, which can
  only add conflicts.
- **The record has no conflict ranges** (section 8 empty): an execution writes no range.

## What group 7 should expect

A `Prepared` per read-write transaction: its `Executed` (the writes `apply_round` takes, at the version the commit
step gives), its `ReadSet` sealed (each table sorted and deduplicated, ready to merge-join against the history, the
catalog range always there, with the namespace and read version), and its record in the `WriteSetRecord` layout:
section 6 the conflict points, 7 the conflict statements, 8 none. `into_parts` gives back the `Executed` and the
`TxnBuffers` for the worker's next transaction. Validation stays exact: every key is held whole, never by a
fingerprint.
