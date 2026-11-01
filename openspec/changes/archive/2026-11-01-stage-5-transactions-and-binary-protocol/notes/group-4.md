# Group 4: read sets at statement granularity

## 4.1 Scenarios

- `ultragraphdb_store_apply/features/changesets/changesets.feature` gained changesets' "An insert records the statement
  it tested", verbatim, beside "Written regions are reported", which was already there.
- `ultragraphdb_store_apply/features/transactions/read_sets.feature` holds the four scenarios of transactions' "What a
  transaction's reads record and what conflicts with them", verbatim, on the recording side: each transaction executes
  against the snapshot at the newest timestamp through a `QuadWrite` over a request of its own (no batch, nothing
  stacked), records its reads, and is judged by the requirement's table, written out in the steps
  (`tests/transaction_steps.rs::conflicts`): a list read whole against any write to it, a tested statement against a
  write adding or removing that object in that list, a scanned range against a write of a key inside it, an absent
  dictionary key against a commit creating it. The judge is the steps' own, not the commit step's resolver, which is
  group 7's; group 7 binds the scenarios again against the resolver. The writes judged are taken from the committing
  transaction's deltas and dictionary entries (the two insert scenarios) or from the store itself, every key holding a
  version above the scanning transaction's read version and every dictionary key new since (the two range scenarios),
  so the judge does not read the implementation's write set.
- The terms named in the two insert scenarios are made known first (`<a> <knows> <z>`, `<b> <p> <c>`): two
  transactions both creating `<a>` would rightly conflict on its absent dictionary key, which is not what the scenario
  is about.
- `true` in `<x> <seen> true` is read as `xsd:boolean` by the step helper `term`, which gained the bare `true` and
  `false`.

## 4.2 Phases 1 to 3

### Surface

- `ultragraphdb_store_posting::Request`: `get_tested(key, object)` (the list read as `get` reads it, the statement
  recorded and not the list), `tested()`, `scan(start, end)` (the keys of a range at the read timestamp and those the
  request wrote, the range recorded), `record_range(start, end)` (a range the caller scanned through the store itself),
  `ranges()`. `read_keys()` now names the lists read whole only. A held list carries whether it was read whole.
- `ultragraphdb_store_quads`: `QuadWrite::holds(graph, subject, predicate, object)`, the statement IDs of a held
  statement or `None`, recording the statement through `get_tested`; `WriteTerms::absent()`, the dictionary keys a
  write looked up and did not find.
- `ultragraphdb_store_apply`: `Reads` (the lists read whole, the statements tested, the ranges scanned, the dictionary
  keys found absent, the namespace and its catalog range) and `Applied::reads()`.

No error type was introduced or extended.

### Decisions taken here, for review

- **Dictionary absences are recorded in `WriteTerms`**, the terms of one write, which `QuadWrite` holds: the task's
  "dictionary absences in `QuadStore`" read as the quads crate, since `QuadStore` is shared by every write and a read
  set is one transaction's. Recorded: an IRI `iri` allocates a UID for (it found the IRI absent first) and one
  `find_iri` does not find. A blank node is skolemized without a lookup and records nothing; a literal's strings are
  written blind, equal strings writing equal entries; a literal's datatype IRI is looked up like any IRI.
- **Which reads become statements.** `ChangesetWrite::held`, the existence check of an insert and of an edge's
  insert, tests the statement for a predicate that is not single-valued; a single-valued predicate's insert reads the
  list whole, since it removes the subject's other objects (an overwrite). Deletes, delete-by-pattern, statement
  deletes, unique checks (the object's reverse list), count indexes (both lists) and the vector index keep their
  whole-list reads, as the design says.
- **A list read from the store still counts in `Applied::lists_read`** and its region in `Applied::read`, whether it
  was read whole or to test a statement: both are store reads, and the bench's figure is a count of store reads.
- **The declaration checks' ranges are exact, not the namespace's whole forward keyspace.** `PredicatePrefix::graphs`
  is a skip scan; `graphs_read` reports what it read: from each seek point to the first key found (inclusive), the
  predicate's prefix in every graph visited, and from the last seek point to the end of the namespace's keyspace when
  nothing more is found. A forward key of the predicate written in any graph, one the scan found, one between, or past
  the last, lies in one of them, and a key of another namespace in none (`reads_tests::
  test_a_declaration_checking_its_data_records_the_ranges_it_scanned`). `forward_lists` records them and scans each
  graph's prefix through `Request::scan`.
- **`QuadStore::commit`'s batch rule** (a write that read a list a refused write of its batch wrote is refused) now
  counts tested statements as reads of their list: without it, a write testing a statement an earlier refused write of
  the batch inserted finds it held through the stacked delta, skips its insert, and commits without the statement.
  Transactions leave the batch path in group 5; the rule stays for the batches that remain.
- **Ranges are a start, inclusive, and an end, exclusive or none**, the shape of `Scan::range`, kept as pairs: no new
  type.

### Corner cases, enumerated before the suite

| Case | Test |
|---|---|
| A statement tested on a list never read, read before, read after, written blind before | `request_tests::test_a_tested_statement_is_recorded_and_its_list_is_not_read_whole`, `test_a_test_after_a_whole_read_keeps_the_list_read_whole`, `test_a_list_read_whole_after_a_test_is_a_read_key`, `test_a_test_completes_a_list_written_blind` |
| One statement tested twice; statements out of key order | `test_tested_statements_are_recorded_once_in_order` |
| A range with keys stored, written, both, at its exclusive end; without an end; empty (start = end) | `test_a_scan_returns_stored_and_written_keys_of_its_range_and_records_it`, `test_a_scan_without_an_end_runs_to_the_last_key`, `test_an_empty_range_finds_nothing_and_is_recorded` |
| A key committed after the read timestamp; a list emptied by the request; a list written and cancelled out | `test_a_scan_reads_the_store_at_the_read_timestamp`, `test_a_list_written_and_cancelled_out_is_not_found` |
| A range recorded twice, recorded and scanned | `test_a_recorded_range_is_reported_once_with_the_scans` |
| A fork's tests, ranges and whole reads absorbed | `test_absorbing_a_fork_keeps_its_tests_ranges_and_whole_reads` |
| A statement held, held with statement IDs, not held beside others, of a subject the write created, inserted or deleted by the write itself | `quad_write_tests::test_a_held_statement_*`, `test_a_statement_not_held_is_recorded_too`, `test_a_subject_the_write_created_records_no_test`, `test_a_test_sees_the_writes_own_insert_and_delete` |
| An IRI known, new, looked up twice, created then found; a lookup finding nothing; a blank node; a datatype | `write_terms_tests` |
| An insert into a multi-valued, single-valued, unique and count-indexed predicate; a delete; a delete of a term the store lacks | `reads_tests` |
| A declaration narrowing a predicate: keys in a graph holding it, one holding only another predicate, between graphs, past the last, at `u64::MAX`; another namespace | `reads_tests::test_a_declaration_checking_its_data_records_the_ranges_it_scanned` |
| A declaration with no data check | `test_a_declaration_without_a_data_check_records_no_range` |
| The catalog range of namespace 0 and 255 (the carry) | `test_the_catalog_range_is_the_namespaces` |
| An empty changeset, on a new namespace and after | `test_an_empty_changeset_reads_nothing` |
| A batch write testing what a refused write of its batch inserted | `quad_store_tests::test_a_write_testing_a_list_a_refused_write_wrote_is_refused` |

Independent sources: catalog-range bytes written out from the key layout (`0x01`, the namespace big-endian, the next
namespace for the end); dictionary keys from `iri_key`, the dictionary's own documented key function; forward and
reverse keys built with `QuadKey`, the key layout pinned by its golden vectors; the range tests' expectations from
keys sorted by their big-endian subject.

### Phase 2: the failing run

`cargo test --no-fail-fast --test unit --test bdd` against the surface:

- `ultragraphdb_store_posting` unit: 249 tests, 12 failed: the 12 new ones, each at an `unimplemented!()` of
  `Request` (`get_tested` 6, `scan` 5, `record_range` 1).
- `ultragraphdb_store_quads` unit: 24 tests, 10 failed: 10 of the 11 new ones, at `QuadWrite::holds` (6) and
  `WriteTerms::absent` (4). The one that passed, `test_a_delete_reads_the_subjects_list_whole`, guards behaviour that
  does not change.
- `ultragraphdb_store_apply` unit: 114 tests, 13 failed: the 13 new ones, each at `Applied::reads`. BDD: 76 scenarios,
  5 failed: the 5 new ones, at `Applied::reads` (1) and `Request::get_tested` and `scan` (4).
- No compile error and no panic elsewhere.

### Phase 3: the audit

The implementation was written against the audited suite and kept (decided 2026-10-27), each defect applied alone,
the owning crate's unit and BDD targets run, then reverted (scripted; the script stayed in the session's scratch
space). One expectation of the suite was wrong before any defect: `test_an_empty_changeset_reads_nothing` ran on a new
store, where the first changeset of a namespace looks `rdf:type` up for the initial catalog and rightly records it
absent; the test now asserts that, then that a second empty changeset reads nothing.

| Defect | Caught by |
|---|---|
| D01 a test reads its list whole | 7 unit tests (`test_a_tested_statement_is_recorded_and_its_list_is_not_read_whole`, `quad_write_tests::test_a_held_statement_is_found_and_recorded_as_a_statement`, `reads_tests::test_an_insert_records_the_statement_it_tested_and_not_the_list` among them); scenarios "An insert records the statement it tested", "Inserts of different objects into one list both commit" |
| D02 a test records no statement | 12 unit tests; scenarios "An insert records the statement it tested", "Inserts of the same new statement conflict", "An insert reads only the subject's list" |
| D03 a whole read of a list holding writes is not marked whole | `test_the_keys_read_leave_out_lists_written_blind` |
| D04 the read keys every list read, as before | `test_a_tested_statement_is_recorded_and_its_list_is_not_read_whole`, `test_a_test_completes_a_list_written_blind` |
| D05 a scan leaves out the request's writes | `test_a_scan_returns_stored_and_written_keys_of_its_range_and_records_it` |
| D06 a scan's end inclusive | the same, `test_an_empty_range_finds_nothing_and_is_recorded` |
| D07 a scan records no range | 4 scan tests |
| D08 a scan reads the newest data | `test_a_scan_reads_the_store_at_the_read_timestamp` |
| D09 absorbing drops the fork's tests | `test_absorbing_a_fork_keeps_its_tests_ranges_and_whole_reads` |
| D10 absorbing keeps only the request's whole-read flag | the same |
| D11 a recorded range is dropped | `test_a_recorded_range_is_reported_once_with_the_scans`, the absorb test |
| D12 `holds` reads the list whole | `test_a_held_statement_is_found_and_recorded_as_a_statement`, `test_a_statement_not_held_is_recorded_too` |
| D13 `holds` tests a created subject's list | `test_a_subject_the_write_created_records_no_test` |
| D14 `holds` reads the write's delta alone | 4 tests, `test_a_held_statement_carries_its_statement_ids` among them |
| D15 a created IRI not recorded absent | 3 `write_terms_tests` |
| D16 a lookup finding nothing not recorded | `test_a_lookup_that_finds_nothing_is_recorded_absent`, `test_absent_keys_are_recorded_once_in_key_order` |
| D17 the batch refusal ignores tests | `test_a_write_testing_a_list_a_refused_write_wrote_is_refused` |
| D18 the catalog range runs to the last key | `test_the_catalog_range_is_the_namespaces` |
| D19 a single-valued insert tests instead of reading the list | `test_an_overwrite_of_a_single_valued_predicate_reads_the_list_whole`, `test_a_single_valued_overwrite_moves_reverse_and_counts`; three overwrite scenarios |
| D20 every insert reads the list whole | 15 unit tests; three scenarios |
| D21 the skip scan's gaps not recorded | `test_a_declaration_checking_its_data_records_the_ranges_it_scanned` |
| D22 its last range not recorded | the same |
| D23 a graph's prefix recorded only where the predicate holds a key | the same, after widening (below) |
| D24 the forward lists record no range | the same |
| D25 the lists read count only lists read whole | `test_a_tested_list_counts_as_read_from_the_store`, two more; "An insert reads only the subject's list" |
| D26 the read set drops the absent keys | 3 `reads_tests` |
| D27 the read set's statements are its lists | 2 `reads_tests`; "An insert records the statement it tested" |
| D28 the skip scan's gap empty | `test_a_declaration_checking_its_data_records_the_ranges_it_scanned` |

Two defects survived the first run:

- **D23** (a graph's prefix recorded only where the predicate holds a key) was a gap: in the test's graph holding
  another predicate only, that predicate's UID was above `nick`'s, so `nick`'s keys there lay in the gap before the
  first key found and the gap range covered them. The test now writes `early`, whose UID is below `nick`'s, in that
  graph, so `nick`'s keys lie past the key found and only the graph's prefix range holds them; rejected on the rerun.
- **The gap's end one past the key found** (the first D28) was equivalent: a write to the key found changes which graph
  the scan visits only when it is under the predicate's prefix, and then it lies in that graph's prefix range. The gap
  now ends at the key found, exclusive, as the argument allows, and D28 became "the gap empty", rejected.

Input variety: the tests' keys, UIDs and objects differ from each other (distinct subjects 1 to 6, distinct objects per
test, namespaces 0, 3 and 255, graphs on both sides of each found graph); the one coincidence that hid a defect was
D23's predicate order, now covered both ways.

## 4.3 Phase 4

The phase-3 implementation, kept. Counts, equal under both build systems (`cargo test -p` and `bazel test` of each
crate's package):

| Crate | Cargo | Bazel |
|---|---|---|
| `store_posting` | unit 249 (237 before), BDD 32, doc 1 | the same |
| `store_quads` | unit 24 (13 before), BDD 20, doc 0 | the same |
| `store_apply` | unit 114 (101 before), the three workload targets 1 each, BDD 76 (71 before), doc 1 | the same |

The changeset workload on the simulator (`workload_seeds`, `workload_faults`, `workload_replay`) runs every insert,
delete and declaration through the statement tests and recorded ranges under every fault class, unchanged and green.

## 4.4 Phase 5: deferred to 11.8

Mutation testing runs once, at the stage's close (11.8, decided 2026-10-27), over every crate the stage changed; nothing
in this group decodes bytes from a client, a peer or the disk, so it has no fuzz target.
