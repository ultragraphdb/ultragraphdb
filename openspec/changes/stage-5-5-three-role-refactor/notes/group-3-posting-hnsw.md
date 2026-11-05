# Group 3: posting lists, vector index and HNSW on 128-bit IDs

This is one of the two parts of the group's second session. The other part covers `traits_blocks`, `store_quads` and
`store_apply`. This part covers:

- `ultragraphdb_store_posting`'s suite.
- `ultragraphdb_traits_index` and `ultragraphdb_store_hnsw` over `SystemId`.
- The HNSW golden vectors.
- Task 3.5, the HNSW lists' fuzz target.
- The planted-defect audits of `core_terms` and `core_mapping`, and coverage.

Logs, scripts and audit results are in the session scratchpad: `g3p/` for this part, `audit/` for the core audits.

At `HEAD` 6002fd40, the user's snapshot commit made while this part was in progress, this part's earlier state is
already committed. Every count below is from the working tree.

## `ultragraphdb_store_posting`

The previous session wrote the library on 128-bit keys first; this session ported the suite onto it. Its phase 2 was
therefore not a failing run against `unimplemented!()` stubs. To make up for that, the new behaviours were audited by
planting defects in the implementation (below).

**Helpers.** Go's tests number their uids, and `tests/utils/helpers.rs` maps the number `n` to a posting key with
`rid(n)`. `rid(n)` is the ID of type 1, bucket 0 and sequence `n`, as a 128-bit value written from the ID's bit layout.
It keeps Go's order, so a read after `n` is a read after `rid(n)`. The other helpers are:

- `id`/`ids`: the IDs themselves.
- `number`: the sequence back.
- `pack_of`: a non-empty pack.

`key(attr, n)` writes the forward key from its layout: 0x10, then the tenant, the default graph's 16 zero bytes, and
the predicate and subject IDs. Values stay keyed by their fingerprint, below 2^64.

**Rewritten to the new format** (each keeps its behaviour, now checked on the 128-bit form):

- **Golden bytes.** Both posting-format goldens (`test_a_delta_has_…`, `test_a_complete_list_has_…`) are now version 2.
  They are written from the layout, and the checksum comes from python-xxhash 3.6.0 (libxxhash 0.8.2), computed by
  `g3p/golden_posting.py`.
- **Backup bytes.** The backup's uid bytes are now LEB128 of 128-bit deltas, hand-computed in Python.
- **Splits start at 0** (`first_split_start`), not 1, and the start bytes carry their kind byte (1 for a fingerprint, 2
  for an ID). A new test checks that a start which is no ID, or a fingerprint start on a value-index key, is refused.
- **Packs and blocks.** No `UidPack` can hold blocks out of order or a repeated block any more, so those two refusals
  are tested on bytes, by swapping or repeating a stored pack's blocks and recomputing the checksum. The refusal is the
  codec's: `the pack is corrupt: block 1 does not start above the block before it`. A block larger than its pack's
  block size is built from a real block of 5.
- **References and literals never share a key.**
  - The tests of a reference coinciding with a literal's fingerprint now check the two kept apart, the literal first.
  - The rollup keeps the literal beside a pack that holds IDs alone.
  - "A rollup packs a shared uid once" became "a rollup packs references alone".
- **`is_plist_empty`** counts postings, because values live there alone (`test_a_pack_without_blocks_is_empty_unless_a_value_is_kept_beside_it`).
- **`test_large_plist_split`.** A block now closes at the block size whatever the distance between IDs. Go's 63 far-apart
  uids would make one block, which cannot split, so the test inserts 300 references with values of 2 KiB.
- **Notes.** A test that hand-built the key in its error message re-derived Go's `%q` over the 57-byte key. The weigher
  test now counts a 57-byte key.

**A defect found and fixed** (`src/types/list/list_iterate.rs`). `List::iterate` kept Go's uid-0 sentinel: it filtered
key 0 out of both iterators and stopped when neither held a key. Key 0 is now a literal's fingerprint, though rare
(1 in 2^64). A posting keyed 0 ended the iteration and hid every posting after it. Coverage found the branch.

- The fix drops the two `> 0` filters. The `Option`s already mark exhaustion, so the branch that stopped on both-none
  could no longer be reached and was removed, and `PPosting::uid` lost its last caller and went with it.
- `test_a_posting_keyed_0_is_iterated_with_the_postings_after_it` failed before the fix (`[]` against `[0, rid(4)]`).

**Bazel.**

| | Unit | BDD | Doc |
|---|---|---|---|
| Before (lift) | 265 | 33 scenarios, 66 steps | passes |
| First run of the ported suite | 261 passed, 5 failed (all test-data conversions, fixed as above) | 32 of 33 (a `1..=1600` comparison not yet mapped) | passes |
| Final | 270 | 33 scenarios, 66 steps | passes |

Five tests were added: the split-start refusal, the reference keyed by no valid ID, key 0, and the two the audit asked
for.

**Audit.** Defects P01–P10 were planted in the implementation, each alone, and the suite was run under Bazel. The
results are in `audit/posting_results.json` and `audit/posting_rerun_results.json`.

| Defect | Caught by |
|---|---|
| P01 a rollup packing any posting keyed by a valid ID, whatever its type | survivor, equivalent: a value is keyed by its fingerprint, below 2^64, so no posting keyed by an ID is a value's |
| P02 a part's value keys left out of iteration | 8 tests, 1 scenario |
| P03 a value-index list's first part starting at 0 | survivor, gap: no test split a value-index list. Killed by `test_a_value_index_list_splits_from_the_smallest_id` |
| P04 any list's first part starting at the smallest ID | 15 tests, 1 scenario |
| P05 every split start written as an ID | 24 tests, 2 scenarios |
| P06 a value's key required in the pack | 13 tests, 1 scenario |
| P07 a list of postings alone called empty | 7 tests, 1 scenario |
| P08 `bin_split` putting values in the high half | survivor, gap: the one test's predicate was non-monotone, and the binary search landed right by chance. Killed by `test_every_value_falls_in_the_low_half`, with three values before the middle |
| P09 the direct path keeping every pack ID | survivor, equivalent: no value is keyed by an ID, so a pack ID's postings, if any, are references and `reference_in` always holds (decision 10) |
| P10 Go's key-0 sentinel back in `iterate` | `test_a_posting_keyed_0_is_iterated_with_the_postings_after_it` |

## `ultragraphdb_traits_index`

**Surface.** Every uid in the traits and types is now a `SystemId`:

- `CacheType::find`, `LocalCache::find` and `Txn::find` return `Option<SystemId>`, `None` where Dgraph returns 0.
- `VectorIndex::search*` return `Vec<SystemId>`; `search_with_uid*` and `insert` take one.
- `SearchFilter` takes one.
- `SearchPathResult`'s neighbours and path hold them.
- `KeyValue`'s entity is one, and `KeyValue` is no longer `Default`.

The crate depends on `ultragraphdb_core_ids`.

**Phase 2.** The phase-1 bodies were `KeyValue::new` and `entity` and `SearchPathResult`'s neighbour and path accessors.
Under Bazel, 24 tests ran and 6 failed, each on `not implemented`. The test doubles were rewritten to keep each entry's
ID beside its value. A new `tests/utils/helpers.rs` provides `id(n)`.

**Audit.** I01–I04 were all caught:

| Defect | Caught by |
|---|---|
| I01 `neighbors()` returns the path | 2 tests |
| I02 `path_mut()` fills the neighbours | 2 tests |
| I03 `accept_all` accepts nothing | 3 tests |
| I04 `KeyValue` keeps an empty value | 2 tests |

**Final.** 24 tests and the doc test pass.

## `ultragraphdb_store_hnsw`

The layouts follow the vector-index spec:

- **Neighbour matrix.** The row count and each row's length are little-endian u64s. Each ID is the 16 little-endian
  bytes of its 128-bit value. An empty matrix is no bytes. The decoder refuses a length that is not whole words, more
  rows or a longer row than the bytes hold, a missing row, trailing bytes, and an ID that is no valid system ID.
- **Entry point.** The node's ID as its 16 big-endian bytes. `bytes_to_id` replaces `bytes_to_uint64`.
- **Dead list.** A JSON array of strings, each holding 32 lowercase hexadecimal digits. `parse_edges` refuses anything
  else, and `add_dead_node` appends.
- **Keys.** The entry point and the dead list are keyed under the predicate's own ID. Their list kinds (0 and 2) keep
  them apart from a node's neighbour lists (1). The entry's `KeyValue` names the predicate's ID as its entity, where
  Dgraph named entity 1.
- **`VectorKeys`.** Takes a `Graph` and the predicate's `SystemId`.
- **`NOT_A_UID`.** Gone: no system ID stands for Go's uid 0.
- **Error texts.** They name an ID by its 32 hexadecimal digits.

**Phase 1.** Bare `unimplemented!()` was the body of:

- `encode_id_matrix` and `decode_id_matrix`;
- `parse_edges`, `add_dead_node` and `bytes_to_id`;
- `VectorKeys::entry`, `dead` and `node_of`;
- `PersistentHnsw`'s `entry_value`.

Everything else changed types only.

**Phase 2.** Run under Bazel: 106 tests, 61 failed. All 7 scenarios failed and the doc test failed, every failure on
`not implemented`. Logs: `g3p/hnsw_phase2_unit.log` and `g3p/hnsw_phase2_bdd.log`.

**Tests written new or rewritten to the layout:**

- **Matrix:** a hand-written hex matrix; Go's matrices encoded to an encoder written apart from the codec.
- **Entry key:** a hand-written key.
- **Dead list:** an enumerated acceptance list and refusal list (31 and 33 digits, uppercase, unquoted, decimal,
  empty, comma errors, nesting, `null`, a stray bracket, an invalid ID), and the round trip.
- **Small helpers:** `bytes_to_id`.
- **`insert_uids_top_k`:** without notAUid.
- **Scenarios:** the vector-index scenarios of task 3.2, verbatim as Gherkin in
  `features/vector-index/vector_index.feature`.
  - "The first insert creates the entry point" and "Deleting a vector marks it dead" have their new text.
  - "Vector lists stay within their graph and predicate" is new: two indexes in graphs 1 and 2, every vector-index key
    scanned, and searches.
  - "A vector of another dimension is refused" belongs to `ultragraphdb_store_apply`'s features, where a declaration
    states its dimension (the other part).

A fork converted the four `persistent_hnsw` test files. It removed the notAUid case of
`test_a_node_without_its_vector_appends_dedupes_and_truncates` (`[0, 6, 6, 12, 15, 3]` became `[6, 6, 12, 15, 3]`, with
the same result), re-derived the offsets of the corruption test for the 72-byte matrix `[[1, 2], [3]]`, and added the
reserved-bit and `bytes_to_id` refusals.

**Audit.** H01–H25 were planted in the implementation. In the first run 23 were caught and 2 survived:

- **H09: a dead-list ID of 31 digits accepted.** The suite's 31-digit case was no valid ID anyway. Two cases were added,
  a valid ID's value with a leading zero cut and one added.
- **H22: a new ID repeated, or already held, inserted again.** A case was added: edges `[4, 9]` with new `[9, 5, 5]`
  gives `[4, 5, 9]`.

Re-run against the widened suite, both now fail. Results are in `audit/hnsw_results.json` and
`audit/hnsw_rerun_results.json`.

| Defect | Caught by |
|---|---|
| H01 a matrix's IDs written big-endian | 42 tests, 6 scenarios |
| H02 a row's length written as u32 | 46 tests |
| H03 the decoder reads IDs big-endian | 42 tests |
| H04 the decoder skips an invalid ID | `test_corrupted_index_values_are_refused` |
| H05 bytes after the last row accepted | the same |
| H06 a row's bound counting 8-byte IDs | the same |
| H07 the word-length check dropped | the same |
| H08 an uppercase digit accepted | `test_parse_edges_refuses_what_is_no_array_of_valid_ids` |
| H09 31 digits accepted | the same, after the cases added |
| H10 an unquoted element accepted | the same |
| H11 an invalid dead-list ID accepted | 7 tests |
| H12 the dead list in uppercase | 5 workload tests |
| H13 the dead list's IDs unquoted | 8 tests, 2 scenarios |
| H14 the dead list unpadded | 8 tests, 2 scenarios |
| H15 a 17-byte entry read from its first 16 | `test_the_small_helpers` |
| H16 the entry under the neighbour lists' kind | 7 tests |
| H17 the dead list under the entry's key | 38 tests |
| H18 `node_of` ignoring the graph | `test_node_of_reads_back_its_own_forward_lists_only` |
| H19 the entry's entity the node | 7 tests |
| H20 the entry little-endian | 39 tests |
| H21 dedupe keeping repeats | `test_a_node_without_its_vector_appends_dedupes_and_truncates` |
| H22 a repeated new ID inserted again | `test_insert_uids_top_k_drops_repeats`, after the case added |
| H23 the error naming the ID in decimal | `test_texts_are_dgraphs` |
| H24 the dead list read from the entry's key | 36 tests |
| H25 no node found read as no value | `test_an_empty_index_reports_the_empty_index_result` |

**Final under Bazel.** 106 tests; 7 scenarios and 14 steps; the doc test passes.

## HNSW golden vectors

No golden file was deleted.

- **`hnsw_graph.json`: no case retired.** Go's uid `n` is the ID numbered `n`, which keeps Go's order. The graph,
  every search result and every insert's values are then Go's, once Go's values are translated to the 128-bit layouts:
  - matrix words to IDs;
  - the entry point's 8 bytes to the ID's 16;
  - the dead list's decimal uids to hexadecimal;
  - entity 1 to the predicate's ID.

  The translation is written apart from the codec, in `persistent_hnsw_golden_tests.rs` (`go_value`, `go_edges`,
  `go_contents`, `go_result`). All 10 cases pass in `test_graph_runs_equal_gos`, all 6 32-bit cases through the posting
  lists, and `cosine` on fjall. No case's matrix holds uid 0.
- **`hnsw_matrix.json`: Go's 14 encodings retired.** Their u64 words are no system IDs. The case `rows` is retired
  because it holds uid 0. The other 13 matrices are kept as inputs, encoded and checked against the layout.
- **`hnsw_parse_edges.json`: all 32 cases retired.** They test Go's decimal format, which the hexadecimal dead list
  replaces. The file is kept on disk and no test reads it; its cases are replaced by the enumerated cases above.

## Task 3.5: fuzz

- The package is `fuzz/ultragraphdb_store_hnsw` and its target is `hnsw_lists_decode`. Each input goes to the matrix
  decoder, `bytes_to_id` and `parse_edges`. Nothing may panic, and whatever decodes must re-encode to its bytes:
  - a matrix to its own bytes, except that no rows comes from no bytes or from a zero count;
  - an entry point to its ID's bytes;
  - a dead list, written again, to the same IDs, and a refusal names the text without its outer spaces.
- The fuzz workspace's `Cargo.toml` gained the crate's path.
- There are 13 seeds, written from the layouts, in `ultragraphdb_store_hnsw/testdata/fuzz_seeds/hnsw_lists_decode/`:
  valid and invalid matrices, entry points and dead lists.
- `cargo +nightly fuzz build --fuzz-dir ultragraphdb_store_hnsw` built the target, and it replayed its 13 seeds
  (`-runs=0`) without failing.
- A 30-second sanity run in a scratch corpus made 1.44 million runs without failing. The time-boxed runs belong to
  group 12.

## Audits of `core_terms` and `core_mapping`

A subagent ran them by the same method: 46 defects, 17 in `core_terms` and 29 in `core_mapping`, each alone under Bazel,
each file restored byte for byte. The driver and the results are in `audit/`. Five survived, all gaps, and each now
fails a test that was added:

| Survivor | Test added |
|---|---|
| T03 `as_id` decoding any 16-byte payload | `test_a_literal_whose_16_bytes_are_a_valid_id_names_no_id`: `"-64"^^xsd:decimal`, whose payload is a valid ID |
| T04 `is_literal` from `as_id` | an assertion in `test_an_id_encoding_of_no_valid_id_names_no_id` |
| M05 `below`'s rejection test inverted | `test_a_draw_below_a_bound_is_lemires_from_the_seeds_draws`: values from Lemire's and SplitMix64's published algorithms, seed 42, computed in Python |
| M19 `without_reifiers` not re-sorting | `test_removing_reifiers_reorders_parallel_edges_by_their_properties` |
| M20 edge order ignoring the reifier | `test_parallel_edges_are_ordered_by_their_reifiers` |

`test_a_triple_term_object_is_no_edge_and_its_subject_is_a_node` was also added, for coverage (decision 9).

Bazel, final: `core_terms` 51 tests and 6 scenarios; `core_mapping` 58 tests and 14 scenarios.

## Coverage

`scripts/coverage.sh` was run over the files the change touched since 25c00035, the group-2 commit. The script
`g3p/added_uncovered.py` keeps the uncovered lines the change added.

| Crate | Added lines uncovered | Reason |
|---|---|---|
| `core_terms` | 1, `encoded_term_view.rs:129` (`lang_string`'s unknown tag) | Unreachable: an `EncodedTerm` is built by `id`, `literal` or `read`, `encoded_len` refuses an unknown tag, and `view` sends only language tags to `lang_string` |
| `core_mapping` | 1, `random_source.rs:61` (the fallback after 128 rejected draws) | Probability below 2^-128 |
| `store_posting` | 1, `lists_rollup.rs:105`, the `Ok(rolled)` after `BATCHES_PER_TURN_MAX` turns (the lift's bounded loop) | The queues hold at most `2 × ROLLUP_QUEUE` full batches and one filling, so `take_batch` runs out first unless writers refill the queues during the turn |
| `traits_index` | 0 | |
| `store_hnsw` | 0 | |

Coverage also led to three new tests in `store_posting`: a reference keyed by no valid ID, refused on the iterating path
and in a rollup, and the key-0 test.

## Decisions to review

1. **`find` returns `Option<SystemId>`** in all three traits, where Dgraph returns uid 0. `ViTxn` still reports
   `KeyNotFound` when no vector passes, as Dgraph's `LocalCache.Find` does, so an empty index stays an
   `EmptyTree(KeyNotFound)`.
2. **The entry point and the dead list are keyed under the predicate's ID**, and the entry's entity is that ID, where
   Dgraph used node and entity 1. No constant ID is needed, and the list kind keeps them apart from a node numbered like
   the predicate.
3. **The dead list's format is strict**: only a JSON array of quoted 32-digit lowercase hexadecimal strings is accepted,
   so one list has one encoding. The refusal keeps Dgraph's variant and text, `UintSlice` and "Cannot convert … to uint
   slice", which now misnames what it refuses; renaming it is a small change awaiting your word.
4. **Errors name an ID by its 32 hexadecimal digits** (`… with uid 00010000000000000005000000000000`).
5. **Uid-0 sentinels left in `store_posting`.** `iterate`'s is fixed (above), but a literal whose fingerprint is 0
   still meets Go's uid-0 sentinel in three places:
   - the mutable layer never indexes key 0, so repeated writes are appended;
   - the uid range bounds of `List::uids` (`uid_min`/`uid_max` 0);
   - `iterate`'s after-key 0, meaning "from the start".
   Each has probability 2^-64 per literal; replacing them with `Option`s is a wider change than this wave's.
6. **A list holding only literals never splits.** `should_split` needs more than one pack block, and `bin_split` puts
   every value in the low half, so a huge literal-only list (one subject with very many literal objects of one
   predicate) grows unsplit. Splitting by postings as well would need a design decision.
7. **`checked_uids`' ascending check in `posting_codec.rs` is now unreachable**: `UidPack::new` refuses blocks out of
   order when the pack decodes. It predates this session and was kept; coverage lists it among the pre-existing lines.
8. **Audit order.** The audits planted defects in the finished implementation, as group 2's and the previous
   session's did. They did not plant them in a throwaway implementation before phase 4.
9. **`test_a_triple_term_object_is_no_edge_and_its_subject_is_a_node`** (`core_mapping`, added by the subagent for
   coverage) pins the current behaviour: a triple-term object makes no edge, while its subject stays a node. The doc
   comment does not settle the case; drop the test if that behaviour should stay open.
10. **`reference_in` in `list_uids.rs` always holds now.** It filtered pack IDs shared with values, which can no longer
    happen. It is the previous session's code and was left as it is; it could be removed.

## Other parts and crates affected

`ultragraphdb_store_apply`, the other part's crate, calls `Txn`, `KeyValue` and `VectorIndex`, whose signatures this
part changed.

`AGENTS.md` lists `ultragraphdb_core_ids` among the dependencies of the three crates.

## Files

- **`ultragraphdb_store/ultragraphdb_store_posting/`:**
  - `src/lib.rs` (the doc test), `src/types/list/list_iterate.rs`, `src/types/p_iterator/mod.rs` and
    `src/utils/posting_codec.rs` (a comment moved);
  - the suite under `tests/`.
- **`ultragraphdb_traits/ultragraphdb_traits_index/`:**
  - `Cargo.toml`;
  - `src/traits/{cache_type,local_cache,txn,vector_index}.rs` and
    `src/types/{key_value,search_path_result,vector_index_options}.rs`;
  - `tests/`, with `tests/utils/helpers.rs` new.
- **`ultragraphdb_store/ultragraphdb_store_hnsw/`:**
  - `Cargo.toml`;
  - `src/` (errors, `lib.rs`, `types/{layer_entry,persistent_heap_element,search_layer_result,vector_keys,vi_txn,query_cache,txn_cache,persistent_hnsw}`
    and `utils/{helper,matrix}.rs`);
  - `features/vector-index/vector_index.feature`;
  - `tests/`;
  - `testdata/fuzz_seeds/hnsw_lists_decode/`, new.
- **The core crates (subagent):** `ultragraphdb_core/ultragraphdb_core_terms/tests/…/encoded_term_tests.rs`, and in
  `ultragraphdb_core/ultragraphdb_core_mapping/tests/` the files `random_source_tests.rs`, `property_graph_tests.rs` and
  `utils/property_graph_view_tests.rs`.
- **Fuzz:** `fuzz/Cargo.toml` and `fuzz/ultragraphdb_store_hnsw/`, new.
- **Shared files:** `AGENTS.md` (three dependency lists) and `tasks.md` (3.5 marked).
