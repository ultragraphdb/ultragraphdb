Every task group runs in the same order: write the feature scenarios and port the Go tests (they fail), port the
code (they pass), refactor with both green. A group is done when its BDD suite and unit tests pass under Cargo and
Bazel with equal counts.

## 1. Store over fjall → `store_kv`

- [ ] 1.1 Add `fjall` to `[workspace.dependencies]`; create `ugdb_store/store_kv`
- [ ] 1.2 Scenarios: kv-store (versions, reads at a timestamp, snapshot scans, atomic batches, drops, banned namespaces)
- [ ] 1.3 Implement versioned keys, per-version metadata, `get_at`, the per-key version iterator, snapshot prefix and range scans
- [ ] 1.4 Implement write batches at a version with a persist mode, out-of-order and same-version writes, tombstones, prefix drops by point tombstones, drop-all by `Keyspace::clear`, banned namespaces, reads refused below the discard timestamp
- [ ] 1.5 Implement the discard timestamp, its bound by open readers, and the compaction filter; scenarios: garbage collection
- [ ] 1.6 Crash test: kill during a batch, reopen, check all-or-nothing

## 2. Posting lists → `store_posting`

- [ ] 2.1 Port `list_test.go` (40), `mvcc_test.go` read cases, `size_test.go`; scenarios: posting-lists (reads, operations, delete-all)
- [ ] 2.2 Port `List`, `MutableLayer`, `ReadPostingList`, `pickPostings`, iteration, `Uids`, `Value`, `ValueFor`, `Length`, posting identities
- [ ] 2.3 Scenarios: posting-lists (rollups, split lists)
- [ ] 2.4 Port rollup, encoding, `shouldSplit`, `binSplit`, lazy part reads, `TxnWriter`

## 3. The local commit → `store_posting`

- [ ] 3.1 Scenarios: local-commit
- [ ] 3.2 Port the request's working set (`LocalCache.plists`) and scratch layers; drop the pending-transaction machinery listed in the design
- [ ] 3.3 Implement commit as one batch at the commit timestamp with the applied timestamp; the single writer; the read gate at `t`
- [ ] 3.4 Implement the local timestamp counter and the oldest-open-read bound used by rollups and the discard timestamp

## 4. The cache → `store_posting`

- [ ] 4.1 Add `moka`; scenarios: posting-cache; port the cache cases of `mvcc_test.go`, adapted from ristretto timing to moka
- [ ] 4.2 Implement the cache of immutable lists keyed by key, updated under the writer lock, its validity rule, and fills refused after a newer commit
- [ ] 4.3 Run the cached-versus-uncached differential test under concurrent commits

## 5. Rollups → `store_posting`

- [ ] 5.1 Port `TestRollupTimestamp`, `TestIncrRollupGetsCancelledQuickly` and the rollup trigger cases
- [ ] 5.2 Port the rollup worker: priorities, batching, deduplication, the pause during rebuilds

## 6. Schema in the store → `core_schema`

- [ ] 6.1 Scenarios: schema-storage
- [ ] 6.2 Port `schema.Init`, `Load`, `LoadFromDb` over `store_kv`; schema and type keys with the discard flag; tombstones

## 7. Applying edges → `store_apply`

- [ ] 7.1 Port `worker/mutation_unit_test.go`, `precalculate_uids_test.go`; scenarios: index-maintenance
- [ ] 7.2 Port `runMutation` and its read strategy, `applyMutations` without Raft (sorting, automatic schema, parallel batches by attribute and entity)
- [ ] 7.3 Port `AddMutationWithIndex`: tokens, reverse edges, counts, reverse counts, delete-all

## 8. Schema changes and drops → `store_apply`

- [ ] 8.1 Port `index_test.go` (18, the `TestNeeds*Rebuild` tables first); scenarios: schema-changes
- [ ] 8.2 Port the decision tables, the dual and interim schemas, `DropIndexes` with split prefixes after a successful build, `BuildData`, the two-phase rebuilder over a temporary store, vector rebuilds in place, the background task gate, type-change refusals
- [ ] 8.3 Port drops: predicate, type, namespace data, everything in a namespace (schema and type keys included), namespace (banned), everything; with split parts removed

## 9. Vector index → `store_hnsw`

- [ ] 9.1 Port `persistent_hnsw_test.go` (15), `kernels_test.go` (7), `ef_recall_test.go` (3); scenarios: vector-index
- [ ] 9.2 Port the storage layout, insert, delete, search with options (the empty-index result), the transaction adapter over the local commit; the dead list in `store_apply`'s index maintenance
- [ ] 9.3 Invalidate the HNSW in-memory caches through the writer

## 10. Close the stage

- [ ] 10.1 No scenario of this stage pending in the traceability test; `scripts/bdd_counts.sh` reports equal counts; `make test` green; `make fix` and `make format` clean
- [ ] 10.2 Measure: batches of Dgraph-shaped commits at 14.5k and 50k commits/s, rollups with the compaction filter over a large predicate, file sizes; record the results in the overview
- [ ] 10.3 Attribution headers on every ported file; `AGENTS.md` crate table updated
