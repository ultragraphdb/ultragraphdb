# Group 3: dictionary, statements and vectors (in progress)

Status at hand-back (2026-11-03): task 3.1 is done; 3.2–3.5 are not. The wave's additions 1–5 are done, 7 is not
reached. `ultragraphdb_store_posting`'s library compiles on 128-bit keys, its suite does not yet; `store_hnsw`,
`store_quads`, `store_apply` and `traits_index` are unchanged beyond the lift. Logs and scripts are in the session
scratchpad (`group3/`).

## 3.1 The Ultra Style lift, each crate its own step

Each lift was made and its suite run under Bazel before any behaviour changed. `core_terms`, `core_mapping` and
`traits_index` were lifted in place. The four store crates do not compile at `HEAD` (group 2 moved their dependencies),
so they were lifted in a scratch worktree at `dfd7368`, where they compile against the old keys and codec (no store
crate changed between `dfd7368` and `25c0003`), and the diff (`group3/lift_store.diff`, 24 files, +77 −30) was applied
to the main tree unchanged. Like group 2's, the lift lands in the group's commit; `group3/lift_small.diff` and
`lift_store.diff` hold it apart.

Bazel before and after, the same counts:

| Crate | Unit | BDD | Other |
|---|---|---|---|
| `core_terms` | 49 | 6 scenarios, 12 steps | — |
| `core_mapping` | 43 | 13 scenarios, 26 steps | — |
| `traits_index` | 24 | — | — |
| `store_posting` | 265 | 33 scenarios, 66 steps | doc test passes |
| `store_hnsw` | 103 | 6 scenarios, 12 steps | doc test passes |
| `store_quads` | 26 | 20 scenarios, 46 steps | — |
| `store_apply` | 198 | 82 scenarios, 232 steps | `workload_faults`, `workload_replay`, `workload_seeds` 1 each; doc test passes |

`cargo fmt -p` and `cargo clippy -p <crate> --all-targets -- -Wclippy::all` clean after. Review of the rules no lint
checks:

- **Rule 1.** A self-call scan of every function (`group3/recursion.py`) found no recursion in the seven crates.
- **Rule 2.** Four `loop`s became bounded: `Lists::run_rollups` rolls up at most `BATCHES_PER_TURN_MAX` (2 ×
  `ROLLUP_QUEUE` + 1, what the queues hold at once) per turn; `List::iterate`'s merge is a `while` over the delta's
  postings and the immutable iterator; `CandidateHeap::down` and `PredicatePrefix::graphs_read` are `while`s naming
  their end. Every other `while` carries a comment saying why it ends. `Lists::gate`'s wait is the one wait without a
  deadline of its own: the transaction limits bound it, and a failed write refuses its waiters (left as is, flagged).
  `parse_vector` stops parsing one element past `VECTOR_MAX`.
- **Rule 5.** Debug checks added on the term encoder (an inline part ≤ 15 bytes, an encoding ≤ 33), the reader (an
  encoding holds its tag), the vector parser, the canonical dataset's statement IDs and the leases.
- **Rule 7.** `EncodedTerm::read`, `as_uid`, `is_literal`, `is_vector` and the whole of `EncodedTerm::view` read
  through `get`/`split_at_checked` instead of indexing; the posting decoder's split and pack-order checks use `first`
  and `is_sorted_by`.
- **Rule 9.** Each `Arc`, `Mutex`, `RwLock`, `RefCell` and atomic in the store crates carries a comment justifying it.
- `List::iterate` grew to 73 lines; its trace of deleted postings moved into `report_deleted`.

## Wave additions

1. **`ultragraphdb_core_catalog`** compiles and passes on the new keys (13 unit, 6 scenarios, the doc test): every
   ID it keys by is a `SystemId` (`declare_*`, `predicate`, `entity_type`, `relation`, `key`, `entry`, `removal`,
   `initial`, `preorder`); no other behaviour changed. Its tests build IDs through a local `id(n)` helper. Group 6 owns
   its real rework.
2. **Split-start kind byte** (`core_keys`, test first). Proposed key-encoding spec wording, replacing the requirement's
   first sentence:

   > A split key SHALL be the base key with the split flag set in its keyspace byte, then a kind byte naming the
   > part's start, then the start: kind 1 and a 64-bit fingerprint as its 8 big-endian bytes for a part starting at a
   > literal, kind 2 and an ID as its 16 big-endian bytes for a part starting at an ID. A value-index key's start
   > SHALL always be an ID, its last 17 bytes, since its token runs to it. A kind byte other than 1 or 2 SHALL be
   > refused with an error naming the key's bytes. Only keys of list keyspaces SHALL be split.

   Suggested scenario: "An ID start cut by 8 bytes is refused: WHEN the split key of a forward key at the start ID
   (type 1, bucket 5, sequence 1000) loses its last 8 bytes and is parsed, THEN parsing fails with an error naming the
   key's bytes." The kinds are chosen so a list's split keys sort as their starts do as 128-bit posting keys (a
   fingerprint below every ID). The fuzz seeds `forward_split_id_start`, `forward_split_fingerprint_start` and
   `value_index_split` were rewritten from the layout; `quad_key_parse` now names `UnknownKind`.
3. **`BucketGroup` in `core_ids`** (`new`, `of`, `get`, `first_bucket`, `BUCKETS`, `COUNT`; `IdError::BucketGroupRange`
   with the text placement had). The file moved with `git mv`, its tests too. Placement keeps the two methods needing
   its own types as `BucketRange::of_group` and `NamespaceRun::of_group`. `ClusterErrorEnum::BucketGroupRange` is
   removed (nothing raises it now) — a change to `ultragraphdb_error_cluster` beyond the two named crates.
4. **groupvarint removed** with `git rm` (`core_codec/src/groupvarint.rs`, `LICENSE-GROUPVARINT`); the codec's
   manifest takes the workspace licence, and the two `go-groupvarint` rows left `ATTRIBUTION.md`. `tools/golden`'s
   `go.mod` keeps Dgraph's indirect dependency on it.
5. **`tools/golden/main.go`** names `algo.go` and `hnsw.go`, which exist.

Also in `core_keys` (task 3.3's regions and group 2's decision 1):

- **The dictionary keyspace**, byte `0x1B`, no lists, no graph: `QuadKey::Dictionary { namespace, entry }` with
  `DictionaryEntry::{Term, Id, Literal}`, kinds 1, 2, 3 and 16 bytes, as term-dictionary's layout states. Its region is
  its keyspace and tenant (quad-storage, "Every key belongs to one region"). The spec leaves the byte open; 0x1B is
  a decision to review.
- **An ID object must be a valid system ID**: a reverse or object-predicates key whose object ID does not decode is
  refused with `InvalidId { field: "object" }`.

## Phases done so far

Phase 1 surfaces (bare `unimplemented!()`), phase 2 runs (every failure the surface's `not implemented` panic or, where
noted, an assertion on the new layout), phase 4 results:

| Crate (change) | Phase 2 | Final |
|---|---|---|
| `core_ids` + `cluster_placement` (`BucketGroup`) | ids 61 passed, 8 failed; placement 48 passed, 7 failed, 1 of 4 scenarios | ids 69; placement 55, 4 scenarios; `error_cluster` 12; `cluster_sequencer` 95, 19 scenarios |
| `core_keys` (kind byte, dictionary) | 46 tests, 28 failed; 13 scenarios, 2 failed | 46, then 47 with the object check (seen failing first); 13 scenarios, doc test |
| `core_terms` (IDs) | 50 tests, 16 failed (14 `not implemented`, 2 hand-built typed-literal bytes in the old 8-byte layout) | 50; 6 scenarios |
| `core_mapping` (reifiers, skolems, random source) | 54 tests, 15 failed (13 `not implemented`, 2 refusal texts now naming the reifier); 14 scenarios, 1 failed | 54; 14 scenarios |

Planted defects (phase 3), each alone, the suite run under Bazel, the file restored (`group3/audit.py`):

| Defect | Caught by |
|---|---|
| G01 a group of 2^18 accepted | `test_a_group_past_18_bits_is_refused` |
| G02 a bucket's group its top 19 bits | `test_a_buckets_group_is_its_top_18_bits`, `test_a_groups_first_bucket_lies_in_the_group` |
| G03 a first bucket 63 × the group | `test_a_groups_first_bucket_is_64_times_the_group` and 1 more |
| G04 a refusal naming another value | `test_a_group_past_18_bits_is_refused` |
| G05 a group's range 63 buckets | `test_a_groups_range_is_its_64_buckets`, scenario of the resolved bucket group |
| G06 a group's run from its low byte | `test_a_groups_run` |
| K21 the start kinds swapped | 5 split tests |
| K22 an ID start without its kind byte | 5 split tests, scenario "A split key round-trips" |
| K23 any kind read as an ID | `test_an_unknown_start_kind_is_refused` |
| K24 a value-index fingerprint kind read as an ID | `test_a_value_index_key_reads_its_start_from_its_last_17_bytes` |
| K25 an ID entry written as kind 1 | 5 dictionary and parse tests |
| K26 a literal entry read as a term | 2 round-trip tests |
| K27 dictionary keys said to name a graph | 3 keyspace and prefix tests |
| K28 the dictionary's byte 0x1C | 12 tests |
| K29 a value-index start from its last 16 bytes | 3 split tests |
| K30 an unknown kind's text without the kind | `test_each_error_names_the_keys_bytes` |
| K31 an ID entry little-endian | 7 tests |

No survivor. `core_terms` and `core_mapping` have not been audited yet.

Corrections in phase 4, each test data, not a weakened assertion: `core_keys`' per-field ID offsets shifted when the
dictionary keys were listed before the marks (now listed last); a trailing-start test filled an ID with `0x01` bytes,
an invalid ID, so validation fired first (now a valid ID); two `core_terms` tests hand-built a typed literal with an
8-byte datatype UID and one an all-zero datatype ID (now 16 bytes, a valid ID). The `core_terms` tag table's lengths
(17, `18 + n`, 33) changed with its surface rather than in phase 4.

Golden values, independent of the code: XXH3-64 of the ID encoding `01 0001 00…2A 00002A 000000`,
6006982149053221746, from python-xxhash 3.6.0 (libxxhash 0.8.2); SplitMix64 seeded 0 (`0xe220a8397b1dcdaf`,
`0x6e789e6aa1b965f4`, `0x06c45d188009454f`, the reference `splitmix64.c` outputs) and seeded 42, computed in Python from
the published algorithm (`python3 -I`); every key, ID and skolem byte string written by hand from its layout.

Fuzz: `cargo +nightly fuzz build` of `fuzz/ultragraphdb_core_keys` and `fuzz/ultragraphdb_core_terms` succeeds;
`quad_key_parse` replayed its 19 seeds (`-runs=0`) without a failure. Coverage (`scripts/coverage.sh`) was not run.

## Decisions to review

1. **Statements are named by reifiers in the model.** `Statement` and `PgEdge` carry `reifier: Option<NamedNode>` in
   place of a statement ID; `StatementRef::Id` and `Change::DeleteStatement { id }` became `Reifier { reifier }`;
   `statement_iri` is gone. `StatementId` stays, a `Statement`-type `SystemId` for the store. `CanonicalDataset` names
   its statement IDs `ugdb:statement/{n}`, the model's own names; a store names them by dictionary skolems, so
   comparisons strip reifiers (`PropertyGraph::without_reifiers`). `MappingError::AmbiguousStatement` now says "name
   one by its reifier".
2. **`RandomSource` is a concrete SplitMix64 type in `core_mapping`**, not a trait, so no trait crate is needed; a
   partition seeds it. `Names::mint_skolem(bucket, &mut RandomSource)` draws the 104 bits; choosing the bucket among a
   partition's owned buckets is left to `WriteTerms` (not written). `Names::node_iri` is removed.
3. **`EncodedTerm::id` holds the store's ID**, and the protocol's results hold a hash in the same tag; `check` (peer
   bytes) therefore does not validate the 16 bytes as an ID, and the key parser does.
4. **Posting keys in `store_posting`**: the pack holds references' IDs alone; a part iterates its value keys (below
   2^64, from its postings) before its pack's IDs; `is_plist_empty` counts postings; a split list's first part starts
   at 0, or at the smallest valid ID for a value-index list, whose keys refuse a fingerprint start; `bin_split` and
   `recursive_split` return `Result`. Key 0 stays the "no posting" sentinel of `iterate`, so a literal whose
   fingerprint is exactly 0 (probability 2^-64) would be skipped.
5. The dictionary keyspace byte 0x1B and the removal of `ClusterErrorEnum::BucketGroupRange` (above).

## Not done (for the next session)

- `store_posting`: its suite (187 compile errors in `tests/`, plus tests whose packs held fingerprints), the
  `List::ids` and rollup semantics' own tests, the posting-list feature steps.
- `traits_blocks` (the block-source trait with its `test-utils` double), `store_quads` (IDs from buckets and blocks,
  get-or-create shared by concurrent executions through a map of pending entries, dictionary keys of the 0x1B layout,
  skolem minting with redraw, statement IDs and reifier entries, absent terms by ID, no dictionary key in the read
  set), `store_apply`, `store_hnsw` and `traits_index` over `SystemId` (entry point 16 bytes, neighbour matrix of
  little-endian IDs, dead list in hexadecimal), the HNSW graph vectors' retirement, the HNSW fuzz target (3.5), the
  scenarios of 3.2, audits of `core_terms`/`core_mapping`, coverage.

## Crates not compiling (group 4 and others)

Libraries: `store_posting` tests; `store_hnsw`, `store_quads`, `store_apply` (all group 3 still); and through
`EncodedTerm::uid(u64)` → `EncodedTerm::id(SystemId)`, `as_uid` → `as_id`, `TermView::Uid`/`Datatype::Uid` →
`Id(SystemId)`, `TermError::not_a_literal(SystemId)`: `ultragraphdb_protocol_wire` (library: `ResultTerm::blank_node`,
`ResultTerm::blank`, `ResultTermView::term`; PROTOCOL.md 10.1 has a result's `0x01` hold the IRI's hash) and
`ultragraphdb_client_core` (`test-utils`' `TestServer`); with them `client_tokio`, `client_compio` and the
`protocol_wire` fuzz package. Mapping changes (`StatementRef::Reifier`, `with_reifier`, `without_reifiers`,
`DeleteStatement { reifier }`) reach `tool_test`'s changeset workload. Group 4's list is as group 2 gave it.
