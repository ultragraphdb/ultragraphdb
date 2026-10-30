# Group 8: the vector indexer

`ultragraphdb_store_apply`, `ultragraphdb_core_mapping` and `ultragraphdb_store_hnsw`, with the catalog's encoding of
the dimension in `ultragraphdb_core_catalog`. A transaction writes a vector in its forward list alone; every round
applied queues the vectors it wrote or removed per predicate, in commit order; a pass of the indexer inserts them into
the HNSW lists, or marks their nodes dead, and records the version it indexed through; a search returns only nodes the
HNSW lists hold whose vector is visible at its snapshot. A node's level is drawn from a stream seeded by a hash of the
index's seed, the namespace, the predicate and the node's UID. A vector index declares its dimension, which writes are
checked against.

## 8.1 Scenarios

The vector-index delta, verbatim; every step bound (`fail_on_skipped`).

| Requirement | Scenario | Bound in |
|---|---|---|
| HNSW vectors live in posting lists (modified) | "A vector of another dimension is refused" (new text: the dimension declared) | `store_apply/features/vector-index/vector_lists.feature` |
| | "The first insert creates the entry point", "Deleting a vector marks it dead", "Vector lists stay within their graph and predicate" (unchanged text) | the same, the steps now running the indexer's pass after each write; `store_hnsw`'s feature keeps its own two |
| Index parameters (modified) | "The exponent sets the parameters" (now "declared with dimension 4") | moved from `store_hnsw` to `store_apply/features/vector-index/vector_index.feature`: a declaration states its dimension, which the HNSW crate does not know; the step declares through the applier and opens the index from the catalog's declaration |
| | "An HNSW option below 1 is refused at the declaration" (now "of dimension 4"), "A declaration without a dimension is refused" (new) | `vector_index.feature` |
| Vectors are indexed in commit order, outside the transaction (added) | "Concurrent vector writes do not conflict", "A deleted vector is not returned" | `store_apply/features/vector-index/vector_indexer.feature` |
| Level draws are a function of the node (added) | "Insertion order does not change levels" | the same |

"Concurrent vector writes do not conflict" runs as the crate's other read-set scenarios do: eight changesets execute
against one snapshot, with an index already holding a vector, every ordered pair is judged by the commit step's rules
taken wider (any key one wrote inside a list another read whole, a statement it tested, a range it scanned, its catalog
range or a dictionary key it found absent), none conflicts, all eight are written as one round at their versions, and
after the indexer's pass a search finds each. Before this group each of the eight read the entry point's list. The
commit step's resolver itself is the commit crate's; binding the scenario through the pipeline waits for the
pipeline to drive the indexer (Open questions).

"Insertion order does not change levels" observes a node's level as the first level at which its neighbour lists hold
a node, which is the level it drew for every node but the entry point; the two runs' entry points, nodes 1 and 10, are
left out, the eight others compared, and the step checks that the levels compared are not all the bottom one and that
the two runs' graphs differ.

## 8.2 Phases 1 to 3

### Surface

| Item | What it is |
|---|---|
| `VectorIndexSpec::with_dimension(NonZeroU16)`, `dimension() -> Option<NonZeroU16>` (core_mapping) | The dimension a declaration states, 1 to 65,535 as a vector literal's; `new(factory, options)` unchanged, stating none |
| `LevelDraw::of_node(seed, namespace, predicate, uid)` (store_apply) | SplitMix64 seeded with the hash of the four: SplitMix64's output on the seed, then on that output XORed with the namespace, the predicate and the UID in turn. `new` and `next_f64` unchanged |
| `ChangesetApplier::index_vectors(namespace, predicate) -> Result<u64>` | One pass of the indexer of a predicate: the version indexed through, the watermark it read at |
| `ChangesetApplier::indexed_version(namespace, predicate) -> Result<u64>` | The version the last pass recorded; 0 before the first |
| `ChangesetApplier::search_vectors(namespace, graph, predicate, query, k, &VectorIndexOptions<f32>, read_ts) -> Result<Vec<u64>>` | The search at a snapshot |
| `ChangesetErrorEnum::NoDimension { predicate }`, `NoVectorIndex { predicate }`; `Dimension { predicate, declared, written }` (was `held`) | "Predicate <p> declares a vector index without its dimension"; "Predicate <p> has no vector index"; the text of `Dimension` unchanged |

Inside `store_apply`: `ChangesetWrite` loses its level draws and its HNSW writes (`index_vector`, `unindex_vector`,
the dead-list helpers move to the indexer); `ChangesetCore` holds the seed and a `VectorQueue` per vector predicate
(`types/vector_queue.rs`), fed by `write_round`; `changeset_core_vectors.rs` holds the pass and the search. The build
draws each node's level by `LevelDraw::of_node` too. `ultragraphdb_core_catalog`'s entry format is 2: a vector index's
dimension follows its factory, u16 big-endian, 0 for none. `ultragraphdb_store_hnsw` changes in its features only. No
dependency added.

### Decisions taken here, for review

- **The dimension is a field, optional in the type and required at the declaration.** `VectorIndexSpec::new` keeps its
  signature, so the wire's tests and the catalog's codec compile unchanged; `declare_predicate` refuses a vector index
  without one (`NoDimension`), before its options are checked, and the catalog holds nothing of it. `NonZeroU16` makes
  a dimension of 0 unrepresentable, and the catalog encodes "none" as 0.
- **The write check reads the declaration**, not the entry point (D8): the index's first vector is checked as every
  other, in every graph.
- **The notification is the applier's round.** `write_round`, the one path every round takes (a transaction's, an
  operation's, a replayed record's, `apply`'s), queues each forward list an execution wrote of a predicate its catalog
  declares with a vector index, as (version, graph, node), after the batch is written. A transaction pays a key parse
  per written list and nothing else; the queue's lock is taken only when a vector is found.
- **A pass** takes the predicate's queue under the apply lock, reads every node's vector at the watermark, inserts it
  (out of the dead list first) or marks the node dead, once per node in commit order of its first change, and commits
  the request at the next version, as `apply` does; nothing pending writes nothing and takes no version. It records
  the watermark it read as the version indexed through. A predicate whose vector index is being built is left to the
  build, whose swap inserts the nodes writes touched; its queue waits, and a later pass inserts those nodes again,
  which changes nothing found. A predicate no longer declared with a vector index loses its queue and is refused.
- **A deleted vector is marked dead** whether or not the index ever held its node, as Dgraph marks it.
- **The search** opens the declared index at its graph and reads at `read_ts`: the HNSW search reads every vector at
  that snapshot and skips a node without one, so a node whose delete is not yet indexed is not found. The one node a
  search can reach outside the lists is its start: the forward lists' first vector when the entry point lost its vector
  or the index has none; a result the lists do not hold at the snapshot is dropped, so a vector not yet indexed is
  absent. An empty index finds nothing, without error; an unknown graph likewise.

### Corner cases, enumerated before the suite

| Case | Test |
|---|---|
| A declaration without a dimension; with 768; 65,535; a dimension replaced | `test_a_vector_index_without_a_dimension_is_refused_and_not_held`, `test_a_vector_index_s_dimension_is_held_in_the_catalog`, `vector_index_spec_tests` |
| A vector shorter and longer than declared, the index's first, in a named graph | `test_a_vector_of_another_dimension_than_declared_is_refused_naming_both` |
| The dimension's encoding: 768 as `03 00`, none as `00 00`; the format before it | `catalog_codec_tests` |
| A transaction inserting, overwriting and deleting a vector: no HNSW key read or written | `test_a_transaction_writing_or_deleting_a_vector_touches_no_hnsw_list` |
| A pass with nothing pending, after a round without vectors | `test_a_pass_with_nothing_to_index_takes_no_version` |
| Commit order against UID order | `test_the_indexer_inserts_in_commit_order_not_in_uid_order` |
| Deleted, deleted when dead, set again; set then deleted, deleted then set, in one pass | `test_a_vector_deleted_is_marked_dead_by_the_indexer_and_set_again_leaves_the_list`, `test_a_vector_set_and_deleted_before_a_pass_ends_dead_and_one_deleted_and_set_ends_live` |
| Two predicates, two graphs; one changeset with two vectors and another triple | `test_each_graph_and_predicate_is_indexed_on_its_own`, `test_a_changeset_inserting_a_vector_and_another_triple_feeds_the_indexer_once` |
| A predicate without a vector index, an unknown one, to the pass and the search | `test_the_indexer_and_the_search_refuse_a_predicate_without_a_vector_index` |
| A pass while the predicate's index is built | `test_the_indexer_leaves_a_predicate_being_built_to_its_build` |
| A search at an older snapshot; of a deleted ordinary node and a deleted entry point not yet indexed; of an empty index; of an unknown graph; `k` and a threshold | `test_a_search_reads_its_snapshot`, `test_a_vector_deleted_and_not_yet_indexed_is_not_returned`, `test_a_search_finds_nothing_in_an_empty_index_or_an_unknown_graph`, `test_a_search_honours_its_count_and_options` |
| A node's draws: each of the four inputs, two swapped, the largest values, replay | `level_draw_tests` |
| The indexer's draws and the build's | `test_the_indexer_draws_each_node_s_level_from_the_node_s_own_stream`, `test_a_build_and_the_indexer_draw_each_node_the_same_level` |

Independent sources: the draws' literals from a separate SplitMix64 written in Python (Vigna's `splitmix64.c`; the
existing seed-0 vector `0xE220A8397B1DCDAF` checked first); distances from the origin by hand (1, 1, √2, 5) for the
threshold; the entry point named by the commit order the test sets; the catalog compared with the catalog loaded again
from the store.

### Phase 2: the failing run

Written against the surface with `unimplemented!()` bodies, no compile error, run with `bazel test` in two stages on
2026-10-30, so that each crate's failures are seen at its own stubs:

- **16:58–16:59, every stub.** `store_apply` unit: 170 tests, 25 failed: 21 at `VectorIndexSpec::with_dimension` or
  `dimension`, 2 at `LevelDraw::of_node`, 1 at `index_vectors`, 1 at an assertion (a declaration without a dimension
  accepted). `store_apply` BDD: 82 scenarios, 14 failed. `core_mapping` unit: 17 tests, 3 failed at its two stubs.
  `core_catalog` unit: 13 tests, 3 failed: 2 at `with_dimension`, 1 at the format byte (1, not 2). `store_hnsw` BDD:
  6 scenarios, passing (one moved out).
- **17:00, `VectorIndexSpec` and the codec implemented.** `core_mapping` and `core_catalog` pass. `store_apply` unit:
  170 tests, 19 failed: 12 at `index_vectors`, 1 at `search_vectors`, 2 at `of_node`, and 4 at assertions the old
  in-transaction path fails: a declaration without a dimension accepted, the index's first vector of another dimension
  accepted, a transaction reading and writing HNSW lists, HNSW lists present before any pass. BDD: 82 scenarios, 11
  failed: 9 at `index_vectors` (the steps' pass), 2 at assertions (the 512-element vector accepted, the declaration
  without a dimension accepted).

### Phase 3: the audit

The implementation was written, then audited with defects planted one at a time in place by a script, each
restored in a `finally`, against the owning crates' unit and BDD targets (`store_apply`'s for its own code, those and
`core_mapping`'s and `core_catalog`'s for theirs); the four crates' source trees, `testdata/` aside, hashed before
and after each round, equal every time. Three rounds on 2026-10-30: 35 defects at 17:08, then 8 after the gaps were
closed and the handover written, then I3 once more.

| Defect | Caught by |
|---|---|
| L1 the UID left out of a node's hash | both `of_node` tests, the indexer's level test |
| L2 a node's stream seeded by the seed alone | the `of_node` literals, the indexer's level test; "Insertion order does not change levels" |
| L3 52 bits of a draw instead of 53 | `test_the_draws_are_splitmix64_s_outputs_scaled_to_the_unit_interval`, the `of_node` literals |
| D1 a longer vector accepted | `test_a_vector_of_another_dimension_than_declared_is_refused_naming_both` |
| D2 the dimension check skipped | the same; "A vector of another dimension is refused" |
| D3 the error's two dimensions swapped | the same test |
| D4 a vector index without a dimension accepted | `test_a_vector_index_without_a_dimension_is_refused_and_not_held`; "A declaration without a dimension is refused" |
| D5 a dimension given dropped by the spec | `vector_index_spec_tests`, the codec tests, most of `store_apply`'s vector tests |
| C1 the dimension encoded little-endian | `test_a_vector_index_s_dimension_is_two_big_endian_bytes_after_its_factory`, the round trip |
| C2 the dimension decoded as none | the round trip; `test_a_vector_index_s_dimension_is_held_in_the_catalog` |
| C3 the catalog format not bumped | `test_an_entry_of_the_format_before_the_dimension_is_refused` |
| Q1 no round feeds the indexer | every indexer test; the scenarios |
| Q2 every forward list queued, vector index or not | gap, closed by `test_a_vector_written_before_its_predicate_declares_a_vector_index_is_not_queued` |
| Q3 namespace and predicate swapped in the queue's key | the indexer's tests; the scenarios |
| Q4 a graph's vectors queued under the default graph | `test_each_graph_and_predicate_is_indexed_on_its_own`; "Vector lists stay within their graph and predicate" |
| I1 a predicate being built indexed by a pass | `test_the_indexer_leaves_a_predicate_being_built_to_its_build` |
| I2 the queue taken in reverse commit order | `test_the_indexer_inserts_in_commit_order_not_in_uid_order`, the two level tests |
| I3 a node changed twice in a pass inserted twice | **survives**, see below |
| I4 a node set again left dead | `test_a_vector_deleted_is_marked_dead_by_the_indexer_and_set_again_leaves_the_list`; "A vector deleted and set again is live" |
| I5 a deleted vector not marked dead | the two dead-list tests; the dead-list scenarios |
| I6 a dead node listed again | `test_a_vector_deleted_is_marked_dead_by_the_indexer_and_set_again_leaves_the_list` |
| I7 an empty pass commits, taking a version | `test_a_pass_with_nothing_to_index_takes_no_version` |
| I8 the version recorded is the pass's own commit's | `test_committed_vectors_reach_the_lists_and_searches_only_through_the_indexer`, `test_a_search_reads_its_snapshot` |
| I9 the indexed version never recorded | the same and `test_a_pass_with_nothing_to_index_takes_no_version`; "Concurrent vector writes do not conflict" |
| I10 every node draws node 0's levels | the two level tests; "Insertion order does not change levels" |
| I11 namespace and predicate swapped in the draw | the two level tests |
| I12 an index no longer declared keeps its queue | gap, closed by `test_the_queue_of_a_vector_index_removed_goes_with_it` |
| B1 the build draws node 0's levels | `test_a_build_and_the_indexer_draw_each_node_the_same_level` |
| S1 a node the lists do not hold returned | `test_committed_vectors_reach_the_lists_and_searches_only_through_the_indexer` |
| S2 an empty index an error | `test_a_search_finds_nothing_in_an_empty_index_or_an_unknown_graph` |
| S3 a named graph searched as the default graph | `test_each_graph_and_predicate_is_indexed_on_its_own` |
| S4 an unknown graph searched as the default graph | `test_a_search_finds_nothing_in_an_empty_index_or_an_unknown_graph` |
| S5 a search at the watermark, not its snapshot | `test_a_search_reads_its_snapshot` |
| S6 one more result than asked | the count test and every `k = 1` search; the scenarios |
| S7 the options dropped | `test_a_search_honours_its_count_and_options` |
| H1 no handover before a pass's inserts | both handover tests |
| H2 a handover to a node without a vector | both handover tests |
| H3 with no heir, the dead entry point kept | `test_an_index_whose_every_node_lost_its_vector_starts_afresh_from_the_first_insert` |
| H4 a live entry point handed over too | the second pass of `test_the_indexer_inserts_in_commit_order_not_in_uid_order`, written before the round, once planning it showed no test kept a live entry point that is not first in key order |
| H5 the last node in key order taken as heir | `test_a_dead_entry_point_hands_over_to_a_node_the_index_holds` |

Q2 and I12 survived the first round and were closed by the tests named; both were rejected in the second. **I3
survives**: `test_a_node_changed_twice_before_a_pass_is_inserted_once` compares the graph a pass leaves when a node
changed twice, its two changes adjacent and then apart, with the graph of one change, and the graphs are equal with
the duplicate inserted: inserting a node again with the vector it holds left every neighbour list as it was on that
five-node set. The guard keeps one insert per node per pass, a cost the suite cannot see; no argument shows the second
insert is a no-op in general (its search starts from a graph holding the node), so it is left as a survivor for 11.8's
mutation run to settle with a measurement, not entered as an equivalence. The equivalent mutants found while writing
were removed from the code instead: the handover's checks that a candidate is not the entry point and that its key is
a neighbour list's (the vector check and the scan's prefix already exclude both).

## 8.3 Phase 4

- 8.3 is the implementation the audit ran against, kept (decided 2026-10-27). A transaction writes vectors in its
  forward list alone: `ChangesetWrite` checks the declared dimension and records the nodes a running vector build
  touches; nothing in an execution reads or writes the vector-index keyspace, so its read set holds no HNSW key.
  `write_round` queues each applied vector; `index_vectors` is the indexer's pass, `search_vectors` the search.
- **Found in the work: the entry point's handover.** The HNSW insert hands a dead entry point over to the forward lists'
  first vector in key order (the port of Dgraph's `calculateNewEntryVec`, ledger 45). Inside a transaction that set
  held every node the index held; under the indexer it also holds the nodes not yet indexed, the node being inserted
  among them, so a pass could make an unindexed node the entry point, with no lists, and leave the indexed nodes
  unreachable from it. The pass hands the entry point over itself first, once per graph, to the first node in key
  order whose neighbour lists the index holds and whose vector is visible at the watermark, or empties it when none
  is, so the next insert starts the index afresh (`test_a_dead_entry_point_hands_over_to_a_node_the_index_holds`, seen
  failing first with the entry point the unindexed `p`; `test_an_index_whose_every_node_lost_its_vector_starts_
  afresh_from_the_first_insert`). The insert's own handover no longer fires during a pass. The search's start, the
  same scan of the forward lists, can still land on an unindexed node when the entry point's delete is not yet
  indexed: the result is dropped, so a search finds nothing until the next pass in that window; the spec's
  requirements hold, its recall does not.
- Written after the implementation, and seen to reject their defects in the audit instead: the two level tests
  (`test_the_indexer_draws_each_node_s_level_from_the_node_s_own_stream`, `test_a_build_and_the_indexer_draw_each_
  node_the_same_level`, for I10, I11 and B1), the three closing Q2, I3 and I12, and the second pass of the commit-order
  test (H4).

Counts, `bazel test` on 2026-10-30 (Bazel only; Cargo was not run, by the user's rule):

| Target | Count |
|---|---|
| `store_apply` unit | 177 tests (170 at phase 2, then the two level tests, the three gap tests and the two handover tests) |
| `store_apply` BDD | 82 scenarios, 232 steps |
| `store_apply` workloads (`workload_seeds`, `workload_faults`, `workload_replay`) | 1 each |
| `store_hnsw` unit, BDD | 92 tests; 6 scenarios, 12 steps |
| `core_mapping` unit, BDD | 17 tests; 13 scenarios |
| `core_catalog` unit, BDD | 13 tests; 6 scenarios |

`bazel build //...` passes, and `bazel test //... --test_tag_filters=-integration_test` runs 118 targets, all passing.
Clippy runs only through `cargo clippy` (`scripts/fix.sh`) and was not run.

## 8.4 Phase 5: deferred to 11.8

Mutation testing runs once, at the stage's close (11.8, decided 2026-10-27), with `cargo mutants` through
`scripts/mutants.sh`, which has no Bazel form; nothing in this group decodes bytes from a client or a peer. The catalog
entry's decoder, which reads the dimension from the disk, has no fuzz target today; one belongs with the catalog's other
decoders at 11.7.

## Open questions

- **The pipeline does not drive the indexer.** A pass commits at the store's next version, as `apply` and the direct
  drops do: correct for an applier without a commit step (tests, tools, the simulator's workload), wrong under
  `Pipeline`, where every write takes its version from the step (7c) and a pass would collide with the next round's
  version. Under the pipeline today vectors commit and are never indexed. The form 7c gave builds' ends fits: a
  `prepare_index_vectors` making the pass an `Executed`, `Prepared::index_vectors` in `ultragraphdb_transactions_txn`,
  and `Pipeline::poll_vectors` proposing each predicate with work under `Pipeline::SYSTEM_TICKET`, at most one pass in
  flight per predicate. Two choices are not settled by the design: its read set (sealed empty, as a build's end, or the
  predicate's catalog entry and vector-index range, so that a drop, a declaration or a build decided meanwhile aborts
  a pass computed before it, which otherwise rewrites lists the drop removed); and whether the queue is taken at the
  proposal or at its apply (a released proposal loses its nodes in the first). It touches the commit and txn crates,
  outside this group's three. Which group does it, and with which read set?
- **The queue is in memory.** After a restart nothing is queued: the rounds replayed above the store's applied version
  feed it, since replay goes through `write_round`, but vectors the store applied before the crash and no pass reached
  stay unindexed until they are written again. The indexed version is recorded in memory alone, so nothing tells a
  restarted node where to resume. Options: the queue's entries written as versionless entries in each round's batch and
  removed by the pass that indexes them (a write per vector, exact); a reconciling scan at open of each vector
  predicate's forward lists against its neighbour lists (no write cost, a scan per restart); or the indexed version
  persisted with the pass and the log read from it, which requires checkpoints never to pass the slowest indexer.
  Which?
- **The wire states no dimension.** The Declare-a-predicate record of a Write body (`PROTOCOL.md` section 4.6, change record 8)
  carries a vector index's factory and options; the session's conversion (10.2) has no dimension to give
  `VectorIndexSpec::with_dimension`, so every declaration from a client would be refused. Either the record gains an
  argument (a u32 after the factory, 0 for none, matching the catalog), or the dimension travels as an option named
  `dimension`. The protocol is 9c's; the spec needs its fix before 10.2.
- **A declaration over vectors of another dimension.** The write check refuses a vector of another dimension than
  declared, but a declaration over a predicate already holding vectors of another dimension is accepted and built. The
  other contradictions a predicate's data makes of a new declaration are refused (`Contradicted`). Should this one be?
