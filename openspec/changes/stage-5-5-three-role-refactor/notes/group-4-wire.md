# Group 4: `ultragraphdb_protocol_wire` (agent 1 of 4)

The wire crate's half of group 4: the library compiling again on `core_terms`' 128-bit IDs, the Commit result of
`PROTOCOL.md` §6.3 in `OperationResult`, and the logical round record of §12.1 and §12.2, with its fuzz target (4.5).
Out of scope here (group 8): Begin, the session frames, role commands 512–530, the result batch's reifier column, error
codes.

## Compiling again

`core_terms` removed `EncodedTerm::uid` (group 2). The library's three errors were in `result_term.rs` and
`result_term_view.rs`. Per §10.1 a result's IRI (`0x01`) and blank-node (`0x02`) entries hold the 16-byte hash of the
IRI's text or of the blank node's skolem IRI, never a system ID, so `ResultTerm` now holds a literal in the store's
encoding or an IRI or blank node by its `Hash128`: `ResultTerm::iri(text)`, `ResultTerm::blank_node(skolem)`,
`ResultTerm::new(literal)`; `term()` is `Option<&EncodedTerm>` (`None` for an IRI or a blank node) and `hash()` gives
the hash. `ResultTermView` alike. The suite's uses of `EncodedTerm::uid` became `ResultTerm::iri`, the skolem IRI of the
tests took §10.2's form (`…/genid/00002a-0123456789abcdef0123456789`), and `bodies_steps.rs`' dictionary helper took the
store's new `QuadStore::open(lists, blocks, random)` and a `SystemId` predicate (dev-dependency
`ultragraphdb_traits_blocks`; dependency `ultragraphdb_core_ids`, which the record's system IDs need too).

`generate_bodies.py`'s result frame now writes the two hashes; `result.frame` is the one golden frame that changed
(648 bytes, checksum `0x87f7d5e34b9ae087`, body `0xaeead453d05a0ed8`). The fuzz seeds `body/result.frame` and
`body_fields/result.frame` were an older variant of the golden frame (600 bytes, differing from byte 0); they now copy
the golden frame.

Bazel after: unit 272 passed, bdd 23 scenarios (46 steps) passed. This is the baseline of the lift.

## The Ultra Style lift

`cargo fmt -p ultragraphdb_protocol_wire` formatted the three edited test files; `cargo clippy -p
ultragraphdb_protocol_wire --all-targets -- -Wclippy::all` reports nothing. Review of the rules no lint checks:

- **Rule 1, no recursion.** A self-call scan of every function found one: `TermTable::import`, which `StatementCache`
  calls for each parameter of an Execute. Besides recursing, it walked every path of a triple term: a table of `t(k+1) =
  <<( t(k) p t(k) )>>` at the depth bound, 32, took 2³² calls (the golden `operation_deep` has exactly that shape, in an
  Operation, which `import` never reads). It now walks an explicit stack and imports each entry of the source table
  once, within `4 × len + 1` steps, refusing past that; the interning order, subject's parts, predicate's, object's,
  then the triple, is unchanged, so the tables it builds are byte for byte the same.
- **Rule 2, bounded loops.** `KeyValues::iter`'s `from_fn` takes `len`, the entries the section was checked to hold.
  The two `while`s carry why they end: `KeyValues::read`'s cursor grows by at least the 8 bytes of an entry's lengths a
  turn; `StatementCache`'s eviction removes one statement a turn.
- **Rule 7, no panicking indexing.** Every remaining index is a constant into a fixed-size array (`ends[2]` of a
  `[usize; 3]`, a pair of `as_chunks::<2>`, `tables[i]` of an `[_; 8]`, a 16-byte operation), which the compiler bounds;
  every slice of received bytes goes through `get`, `bytes_at` or `chunks_exact`.
- **Rule 9.** No `Arc`, `Rc`, interior mutability or `dyn` in the crate.

Bazel after the lift: unit 272 passed, bdd 23 scenarios passed, the same counts.

## Phase 1: the surface

`SURFACE READY` was appended to `group-4-coordination.md` once the library compiled with every new body
`unimplemented!()`:

- `OperationResult` (§6.3): `for_node(term index, created)`, `counter(i64)`, `compare_and_set()`; `value()`, `node()`
  (`None` for [`ABSENT`]), `created()`, `encode() -> [u8; 16]`. `transactions_commit` maps `store_apply`'s
  `OperationOutcome::Node { node, created }` by interning the node's IRI or skolem IRI (`WriteSetRecord::push_node_result`
  does both), `Set` to `compare_and_set()` and `Counter(v)` to `counter(v)`.
- `RecordKind` (1 transaction or Drop, 2 a home's registration, 3 a build), `RecordOperation` (codes 1–7: drops of a
  predicate or graph by `SystemId`, of a tenant's data, of everything in it; a build's start and completion by catalog
  version; code 5 is unassigned), `RecordRegistration` (`SystemId`, `BucketGroup`, `NonZeroU64` generation), each with
  `encode()` to its fixed bytes, so a builder writing straight into a lent buffer (`WriteSetBuilder`) need not allocate.
- `WriteSetRecord`: `transaction(ns)`, `registration(ns, registration)`, `build(ns, operation)`; setters for catalog,
  read version, session (16 bytes) with its request, principal, commit record; pushes for each section, and
  `push_node_result(iri, created)`; `encode()`. `WriteSetRecordView::new` checks in §12.2's order and reads every
  section in place; `to_record()`.
- `RoundHeader::new(position, parent, incarnation, first_version, transactions, derivation: NonZeroU32)`;
  `decode(&[u8; 128])` is now fallible (unused bytes 40–127 zero, then derivation ≥ 1). `RoundBody::new(…, derivation)`,
  `insert_dictionary(key, value)`; `RoundView::dictionary()`.
- Constants `RECORD_FIXED_SIZE` 64, `RECORD_SECTIONS` 12, `RECORD_OPERATION_SIZE` 24, `RECORD_REGISTRATION_SIZE` 32,
  `OPERATION_RESULT_SIZE` 16.

## Phase 2: the suite, observed failing

`generate_bodies.py` lays out a new golden Round from §12.1 and §12.2 independently of the crate: position 3, first
version `(7 << 22) | 100`, derivation 1, five entries (a transaction of tenant 3 at catalog 11 with every section
filled, three results and a 4,000-byte commit record so the body takes chunks; an abort; a home's registration; a
build's start in tenant 4; a Drop of a predicate), and three dictionary entries. `round.frame` is 5,520 bytes,
checksum `0x9516930d4ee772f5`, body `0xf6853c79e1db4579`. The Commit golden frame's two results took §6.3's layout (a
counter add of −5, a compare-and-set): 296 bytes, checksum `0xa27355ade16c06e6`, body `0xff715c73e704ed19`. Every other
golden frame is unchanged byte for byte.

Bazel, against the surface: unit 293 tests, **231 passed, 62 failed**, every failure the `not implemented` panic
(`grep` of the log: 62 of 62); bdd 23 scenarios passed (no scenario reaches the record). Failing by module: commit 1,
commit_view 4, key_values 4, operation_result 3, record_kind 2, record_operation 3, record_registration 2, round_body 3,
round_header 3, round_view 10, traits 8 (the golden round and commit through every body's checks), write_set_record 6,
write_set_record_view 13.

Corner cases enumerated before the suite, each with its test:

| Case | Test |
|---|---|
| A result naming node 0, the last index below `ABSENT`; a counter of 0, ±1, `i64::MIN`, `i64::MAX`; compare-and-set's bytes equal a counter of 0 | `operation_result_tests` |
| Each kind's byte; 0, 4, 255 naming none | `record_kind_tests` |
| Each operation's code, argument and bytes; codes 0, 8, 65,535; an ID of prefix 0, of sequence 0, of a reserved byte set, all zero; a nonzero argument for codes 3–5; a build's bytes 8 and 15 set; catalog version 0 and `u64::MAX` | `record_operation_tests` |
| A registration's bytes; group 0 and 2¹⁸ − 1; generation 1 and `u64::MAX` | `record_registration_tests` |
| An empty record (fixed part and table alone); node results interned once each, in result order; a later setting replacing the earlier; a build whose catalog differs from its operation's version | `write_set_record_tests` |
| Every refusal of §12.2, in order: too short, trailing bytes, kinds 0, 4, 255, bytes 1 and 7, each field a kind leaves 0 (first and last byte), an entry past its section, padding, a statement ID of 15, 17 and no bytes, 16 and 0 taken, a conflict point with a value and with none, a conflict statement and range with none, operations of 23 bytes, unknown codes, padding at 2 and 7, a bad ID and a bad build argument, registrations of 31 bytes and each of their five breaks, group 2¹⁸ − 1 taken, a literal in the results' table, a text gap, results of 47 bytes, padding, `created` 2, a node past the table, a value beside a node, a creation without one, and each kind's sections (two drops, a Drop inserting, a transaction's build, a transaction beside a drop, a registration of 0 and 2 and with a commit record, a build of none, of a drop, with a registration, with a range, a Drop's read version) | `write_set_record_view_tests` |
| The header's fields at distinct bytes, all ones, all zero; unused bytes 40 and 127; derivation 0, and unused bytes before it | `round_header_tests` |
| The golden round, an empty round, a dictionary key inserted twice | `round_body_tests` |
| Every refusal of §12.1: decisions short and miscounted, outcomes 99 and 65,535, padding, a commit's record and detail, an abort's record; the last commit at sequence 2²² − 2, one past it, into the next epoch; the record table short and miscounted, an offset, past section 2 by far and by one, a gap, padding, trailing bytes, a record refused; one tenant naming two catalogs; dictionary entries past the section, padded, removed, out of order, repeated, a prefix before a longer key; the damaged-range naming over each record and the dictionary | `round_view_tests` |
| A Commit result naming a node beside a value, creating without a node, naming node 3 | `commit_view_tests` |

## Phase 3: inputs reviewed for coincidences

The golden build named catalog 12 in its fixed part and as its operation's version, so a field written into the other
would pass; `test_a_builds_catalog_and_its_operations_version_are_apart` adds catalog 13 against version 12, checked
byte by byte. Every other field of the golden records is distinct from its neighbours (tenant 3 and 4, catalogs 11
and 12, requests 7 and 8, principal 9, read version `(7 << 22) | 50`, a session of 16 distinct bytes), and the round's
abort names code 2 with detail 1.

## Phase 4: implementation

Bazel: unit **296 passed** (293, the coincidence test, one long test split for the 70-line rule, and the import test
below), bdd 23 scenarios (46 steps) passed. `cargo clippy -p ultragraphdb_protocol_wire --all-targets -- -Wclippy::all`
reports nothing.

`traits/body_view_tests.rs`' per-command tables still held Round's old shape (3 sections, header fields 0–36); they
were missed in phase 2 and now hold §12.1's (4 sections, fields 0–40), as the spec states, not as the code does.

`scripts/coverage.sh` over the added and edited files leaves three lines uncovered, each unreachable:
`round_view_body_view.rs`'s `to_body` refusal of a record missing for a commit (a view holds one record per commit, which
`from_body` checked), and `TermTable::import`'s `None` after its loop (the bound is the walk's own count, proven above).
The fourth, `import`'s pop of an entry already imported, is covered by
`test_a_parameter_naming_one_part_at_every_level_imports_each_entry_once`, which also pins the lift's fix: the old walk
took 2³² steps on it.


## Phase 5: mutation testing and the fuzz target

`scripts/mutants.sh ultragraphdb_protocol_wire <file>` over each added or edited file, with `CARGO_MUTANTS_OUTPUT`
pointed at the session scratchpad (`g4/mutants/`) so this run did not share `mutants.out` with the other agents'. First
pass:

| File | Mutants | Caught | Unviable | Missed / timeout |
|---|---|---|---|---|
| `types/operation_result.rs` | 16 | 12 | 4 | — |
| `types/record_kind.rs` | 7 | 6 | 1 | — |
| `types/record_registration.rs` | 9 | 6 | 3 | — |
| `types/record_operation/mod.rs` | 20 | 19 | 1 | — |
| `utils/results.rs` | 18 | 16 | 2 | — |
| `utils/operation.rs` | 22 | 20 | 2 | — |
| `types/round_header/mod.rs` | 19 | 18 | 1 | — |
| `types/round_body/mod.rs` | 20 | 18 | 2 | — |
| `types/round_body/round_body_body_builder.rs` | 5 | 5 | 0 | — |
| `types/round_view/mod.rs` | 21 | 17 | 4 | — |
| `types/round_view/round_view_body_view.rs` | 59 | 49 | 8 | 2 missed |
| `types/write_set_record/mod.rs` | 76 | 68 | 8 | — |
| `types/write_set_record_view/mod.rs` | 76 | 59 | 15 | 2 missed |
| `types/result_term.rs` | 20 | 11 | 7 | 2 timeouts (re-run: 13 caught, 7 unviable) |
| `types/result_term_view.rs` | 24 | every viable one | | — |
| `types/result_view/result_view_body_view.rs` | 31 | 28 | 2 | 1 timeout |
| `types/term_table/mod.rs` | 43 | baseline failed; re-run: 40 | 2 | 1 missed |
| `types/key_values/mod.rs` | 25 | 19 | 6 | — |
| `types/commit_view/mod.rs` | 14 | 13 | 1 | — |
| `types/commit_view/commit_view_body_view.rs` | 2 | 0 | 2 | — |

Survivors and how each was resolved:

- `round_view_body_view.rs:135:60` and `:233:76`, `+ j` → `- j` in the padding offsets of a decision and of the gap
  after a record: every test set the first padding byte (j = 0). Killed by `test_decisions_are_counted_and_well_formed`
  (decision 1's second zero byte) and `test_padding_after_a_record_is_named_at_its_own_byte` (a record ending 2 past a
  multiple of 8, its padding's fourth byte).
- `write_set_record_view/mod.rs:396:13` and `:397:13`, deleting `count`'s arms for sections 9 and 10, which then fall
  through to the text's byte count: no test put results or terms into a kind that takes none. Killed by
  `test_each_kind_holds_its_sections`' build with a counter's result and with the transaction's term table.
- `result_view_body_view.rs:126:5`, `check_cells` → `Ok(())`: a timeout, which is detection: without the check,
  `test_cells_count_rows_times_columns_in_sixty_four_bits` walks 2³² cells.
- `result_term.rs:134:9` (`annotation` → `Some("xyzzy")`) and `:114:9` (`is_blank_node` → `false`): timeouts while the
  machine was loaded (one build took 174 s against a 45 s test timeout); the re-run caught both.
- `term_table/mod.rs`: the first pass's baseline failed in `error_reply_view_tests`, because the copy caught another
  agent's half-done addition of `ErrorCode::CrossPartition` to this crate. The re-run left
  `term_table/mod.rs:134:9`, `TermTable::is_empty` → `true`, a method this change did not touch, which no test asserted
  false; `term_table_tests::test_a_table_holding_a_term_is_not_empty` now does.

The second pass confirmed the kills: `round_view_body_view.rs` 59 mutants, 51 caught, 8 unviable;
`write_set_record_view/mod.rs` 76 mutants, 61 caught, 15 unviable.

### Open, for group 12's task 12.1

- `term_table/mod.rs:134:9`, `TermTable::is_empty` → `true`: `test_a_table_holding_a_term_is_not_empty` was added to
  kill it after the re-run, at the coordinator's wrap-up, so no run has confirmed the kill yet. It survived because
  nothing in the crate calls `is_empty`, and the only test of it asserted an empty table.

The fuzz target of the logical record's decoder is `fuzz/ultragraphdb_protocol_wire/fuzz_targets/write_set_record.rs`,
strengthened: any bytes through `WriteSetRecordView::new`, and an accepted record must encode to the same bytes, hold
what its kind takes (a transaction at most one drop, and with it no statements and read version 0; a registration one
registration and catalog 0; a build one build operation), statement IDs of 0 or 16 bytes, empty conflict points, and
results naming a node below the term table's length with no value, or no node and no creation. Its seeds are the suite's
records: the golden Round's four (`testdata/fuzz_seeds/write_set_record/record0.bin` to `record3.bin`; the first two
overwritten, the others new) and the empty transaction record (`record_empty.bin`). `round.rs` takes the derivation and
asserts the dictionary's ascending keys; its seed `round/round.body` is the golden Round's body. `body_fields.rs` builds
results, result terms, rounds and records through the new API. `cargo +nightly fuzz build --fuzz-dir
fuzz/ultragraphdb_protocol_wire` builds all nine targets, and each seed directory ran once (`-runs=0`) without a
failure. The fuzzing runs belong to group 12.

Final Bazel: unit **299 passed**, bdd 23 scenarios (46 steps) passed; `cargo clippy -p ultragraphdb_protocol_wire
--all-targets -- -Wclippy::all` reports nothing.

## Choices the spec left open

- `FieldValue` values: `"record"` names the field's offset in the fixed part (16 catalog, 24 read version, 32 session,
  48 request, 56 principal); `"argument"` and `"registration"` name the item's index in its section, since a 16-byte
  argument or entity has no u64 value; `"statement id"` and `"conflict point"` give the value's length, or `ABSENT`
  when the value is missing; `"conflict statement"`, `"conflict range"` and `"dictionary entry"` give `ABSENT`;
  `"dictionary order"` gives the entry's index; `"result"` the value's bits, or 1 for a creation without a node;
  `"node term"` the entry's kind.
- `CountMismatch` `"record sections"`: `count` is the first offending section's item count (entries, operations,
  registrations, results, terms; bytes for the commit record and the term text), `expected` what the kind takes, 0 or
  1. After the sections come the operation's sort (`FieldValue` `"operation"`: a build's in a transaction, a drop in a
  build) and then a Drop's read version (`FieldValue` `"record"`, 24). A transaction with one operation is a Drop; with
  more, `(n, 1)`.
- The results' term table is checked before the results, as §6.3 orders it, since a node's bound is the table's
  length; §12.2's sentence lists section 9 first.
- A build's catalog version 0 is taken, and nothing compares a build's version with the record's catalog.
- The Commit body (group 8's) carries results in §6.3's layout but no term table yet, so its results' nodes are not
  bounded; its golden frame holds a counter add and a compare-and-set, valid under §6.3 once the table arrives.
- `RoundHeader::decode` is fallible and checks the unused bytes; the derivation is a `NonZeroU32` in every constructor.
- `RoundBody` keeps its dictionary in a `BTreeMap`, so its entries are ascending and unique by construction; a key
  inserted again keeps the later value.
- Sections 0 and 1's 16-byte values are not checked as system IDs: §12.2 asks only "empty or of 16 bytes".
- The session is `[u8; 16]`: `core_ids` has no session type.

## Flagged

- **No group-4 scenario binds to this crate.** The four scenarios of "A round's record is logical" replay a round into a
  store, so they belong to the crate that replays (`transactions_commit`'s `Replay`). The wire's bdd target is
  unchanged at 23 scenarios.
- **Result literals of other datatypes (`0x32`, `0x33`) still carry the store's datatype ID bytes**, where §10 asks for
  the hash of the datatype's IRI, and `ResultTerm::new` accepts an IRI's ID encoding, which would put a system ID in a
  result. Both are result-batch work and belong to group 8.
- Any client's Execute could reach `TermTable::import`'s exponential walk (fixed in the lift).
- Another agent is adding `ErrorCode::CrossPartition` (code 23) to this crate in parallel; my files do not touch it.
- Cargo's fingerprint of `core_ids` was stale and reported `BucketGroup` missing; touching the mtime of
  `ultragraphdb_core_ids/src/lib.rs` (no content change) cleared it.
- `testdata/golden/round.frame` (the frame-level golden of `generate_frames.c`) is a chunked body with no particular
  layout and was left as is.
