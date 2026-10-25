Every task group runs in the same order: write the feature scenarios and port the Go tests (they fail), port the
code (they pass), refactor with both green. A group is done when its BDD suite and unit tests pass under Cargo and
Bazel with equal counts.

## 0. Store boundary and simulator → `ultragraphdb_error_store`, `ultragraphdb_traits_store`, `ultragraphdb_store_sim`

- [x] 0.1 Add `ultragraphdb_store/*` to the root `Cargo.toml`'s members; create `ultragraphdb_error/ultragraphdb_error_store` (`StoreError(pub StoreErrorEnum)`) and `ultragraphdb_traits/ultragraphdb_traits_store`: the `Store` trait, synchronous and shaped in batches whose entries carry their versions, the types its signatures name, the `Executor` and `ListCache` traits, with no implementation yet; scenarios: storage-simulation, whose posting-list scenario closes with group 2
- [x] 0.2 `ultragraphdb_store/ultragraphdb_store_sim`: the in-memory `Store`, the seed and its derived streams, simulated time, the step scheduler and the stepped `Executor`, the deterministic `ListCache` with seeded eviction, seed replay
- [x] 0.3 The fault classes: a failed batch poisoning the store until recovery, with the batch present or absent after it; process crash; power loss of a suffix of the unsynced batches, prefix-closed; a crash or power loss during a drop; failed read, at once or partway through a scan; corrupted value; slow operation
- [x] 0.4 The shadow model and the invariants checked after each step and each recovery; a fixed seed set as a hermetic Bazel test tagged `unit-test`, and a fuzz binary run outside `make test`
- [x] 0.5 The determinism rule enforced: storage crates take clock, randomness and threads as given; the workspace `clippy.toml` lists `Instant::now`, `Instant::elapsed`, `SystemTime::now`, `thread::spawn`, `thread::Builder::spawn`, `thread::scope` and `RandomState::new` under `disallowed-methods` beside the `mul_add` entries; `#[allow(clippy::disallowed_methods)]` with its reason on the production implementations in `ultragraphdb_store_kv` and on the existing uses in `ultragraphdb_core_algo`, `ultragraphdb_core_chunker` and `ultragraphdb_client`'s test utilities; `scripts/check.sh` runs `cargo clippy --workspace --all-targets -- -D clippy::disallowed_methods`, so `make check` fails on any other use

Each later group adds its operations to the simulator's workload and its rules to the shadow model before it is done.

## 1. Store over fjall → `ultragraphdb_store_kv`

- [x] 1.1 Add `fjall`, pinned to `=3.1.12` for its hidden `major_compact` and `rotate_memtable_and_wait`, to `[workspace.dependencies]`; create `ultragraphdb_store/ultragraphdb_store_kv`
- [x] 1.2 Scenarios: kv-store (key order, key limits, versions, reads at a timestamp, snapshot reads, atomic batches, drops, banned namespaces)
- [x] 1.3 Implement the `Store` trait over fjall: escaped, terminated and versioned keys, the key-size refusals, per-version metadata, `get_at`, the per-key version iterator, snapshot prefix and range scans, every read through `Database::snapshot()`
- [x] 1.4 Implement write batches with a version per entry and at least `PersistMode::Buffer`, a batch of any size as one fjall batch (a test writes 1 GiB), out-of-order and same-version writes, deletions; the `process` and `filesystem` durability modes; a failed batch poisoning the store; physical prefix drops and drop-all by `Keyspace::clear`, both through drop records finished at open, drop-all clearing the banned set, the discard timestamp, the tombstone horizon and the applied timestamp; banned namespaces; reads refused below the discard timestamp with `Transaction is too old`
- [x] 1.5 Implement the discard timestamp, its bound by open readers, the tombstone horizon defaulting to it, the total compaction filter over shared atomics, the deletion sweep and `compact`; scenarios: garbage collection
- [x] 1.6 Crash test: kill during a batch and during a drop, reopen, check all-or-nothing and the finished drop; the writer binary a `data` dependency of the test, its directory under `TEST_TMPDIR`
- [x] 1.7 The thread-backed `Executor`

## 2. Posting lists → `ultragraphdb_store_posting`

- [x] 2.1 Port `list_test.go` (40) and `mvcc_test.go`'s `TestPostingListRead`; port go-farm's `Fingerprint64` into `ultragraphdb_common_utils` as `go_farm` (MIT, from dgryski/go-farm) with golden vectors generated from Go; scenarios: posting-lists (reads, operations, delete-all). `size_test.go` is not ported: it asserts Go's struct sizes, and task 4.1 tests the cache's weigher
- [x] 2.2 Port `List`, `MutableLayer`, `ReadPostingList` with reads below an empty rollup reading the older versions, `pickPostings`, iteration, `Uids`, `Value`, `ValueFor`, `Length`, posting identities
- [x] 2.3 Scenarios: posting-lists (rollups, split lists)
- [x] 2.4 Port rollup, encoding, `shouldSplit`, `binSplit`, part reads with the main key through one snapshot, `TxnWriter`; a rollup's main key, parts and part deletions as one batch at the main key's version
- [x] 2.5 Each Dgraph panic or fatal in the files of this group returns `PostingError`, and its test names the deviation

## 3. The local commit → `ultragraphdb_store_posting`

- [x] 3.1 Scenarios: local-commit, whose rollup scenario closes with group 5
- [x] 3.2 Port the request's working set (`LocalCache.plists`) and scratch layers, stacked across the requests of one batch; a list built without a store read completed before it is read; `GetScalarList` through the merging iterator; drop the pending-transaction machinery listed in the design
- [x] 3.3 Implement commit as part of one batch at the commit timestamp with the applied timestamp, several commits per batch; the single writer, which hands out commit timestamps as it writes; the read gate at `t`
- [x] 3.4 Implement the local timestamp counter and the oldest-open-read bound used by rollups and the discard timestamp
- [x] 3.5 Each Dgraph panic or fatal in the files of this group returns `PostingError`, and its test names the deviation

## 4. The cache → `ultragraphdb_store_posting`, `ultragraphdb_store_kv`

- [x] 4.1 Add `moka`; scenarios: posting-cache; port `mvcc_test.go`'s `TestCacheAfterDeltaUpdateRecieved` and `TestCacheStaleWhenMaxTsLessThanReadTs`, adapted from ristretto timing to the port's cache; `TestReadUidsHonorsPostingListCache` and the nine uid-warming tests are not ported (design, Non-Goals); test the cache's weigher
- [x] 4.2 Implement the cache of immutable lists keyed by key behind `ListCache`, its production implementation over moka in `ultragraphdb_store_kv` (`build_with_hasher` with std's per-process `RandomState`, so crafted keys cannot force collisions), updated under the writer lock, its validity rule, its epoch, and fills refused after a newer commit or an epoch change
- [x] 4.3 Run the cached-versus-uncached differential test under concurrent commits
- [x] 4.4 Each Dgraph panic or fatal in the files of this group returns `PostingError`, and its test names the deviation

## 5. Rollups → `ultragraphdb_store_posting`

- [x] 5.1 Port `TestRollupTimestamp`, `TestIncrRollupGetsCancelledQuickly` and the rollup trigger cases
- [x] 5.2 Port the rollup worker on the `Executor`: priorities, batching, deduplication, the pause during rebuilds, a rollup discarded when a drop covered its key after its read or its key gained a deletion above its version
- [x] 5.3 Each Dgraph panic or fatal in the files of this group returns `PostingError`, and its test names the deviation

## 6. Schema in the store → `ultragraphdb_core_schema`

- [x] 6.1 Scenarios: schema-storage, whose write scenarios run in `ultragraphdb_store_apply`'s features (task 7.1)
- [x] 6.2 Port `schema.Init`, `Load`, `LoadFromDb` over the `Store` trait, run on both implementations; schema and type keys with the discard flag; deletions
- [x] 6.3 Each Dgraph panic or fatal in the files of this group returns `SchemaError`, and its test names the deviation

## 7. Applying edges → `ultragraphdb_store_apply`

- [x] 7.1 Port `worker/mutation_unit_test.go`; scenarios: index-maintenance, and schema-storage's write scenarios
- [x] 7.2 Port `runMutation` and its read strategy, `ValidateAndConvert`, `applyMutations` without Raft (sorting, automatic schema in the request's overlay under Dgraph's creation rules, the first commit's schema checked at commit, parallel batches by attribute and entity on the `Executor`)
- [x] 7.3 Port `AddMutationWithIndex`: tokens, reverse edges, counts, reverse counts, delete-all; a deleted list value keeping the tokens the subject's other values produce; a star delete tokenizing each value with its language and clearing the count key the subject is listed under
- [x] 7.4 Each Dgraph panic or fatal in the files of this group returns `ApplyError`, and its test names the deviation

## 8. Schema changes and drops → `ultragraphdb_store_apply`

- [x] 8.1 Port `index_test.go` (18, the `TestNeeds*Rebuild` tables first); scenarios: schema-changes
- [x] 8.2 Port the decision tables, the dual and interim schemas, the two-phase rebuilder over a temporary store with writes maintaining both schemas' indexes, the swap batch with `BuildData` and a `@count` rebuild's data lists, the cleanup of a failed build, vector rebuilds in place over merged lists, the background task gate, type-change refusals checked before anything changes with exact "holds data"; advance the cache's epoch
- [x] 8.3 Port drops: predicate (a vector predicate's three HNSW predicates included), type, namespace data, everything in a namespace (schema and type keys included), namespace (banned), everything; physical, with split parts removed, and the cache's epoch advanced
- [x] 8.4 Ledger 7 and 8: a namespace's data drop removes its `0x04` parts with its `0x00` data in one drop; the complete namespace drop removes `[0x00,ns]`, `[0x04,ns]`, `[0x01,ns]`, `[0x02,ns]` physically, then writes the namespace's initial schema at ts 1; rollups of a predicate or namespace that read before its drop write nothing after it (ledger 38)
- [x] 8.5 A request whose read timestamp is below a drop or schema change it writes under fails at commit with `Transaction has been aborted. Please retry`
- [x] 8.6 Each Dgraph panic or fatal in the files of this group returns `ApplyError`, and its test names the deviation
- [x] 8.7 A rebuild's temporary entries in a temporary fjall keyspace, dropped after the swap or the failure batch; the simulator and fjall runs cover a rebuild larger than the cache
- [x] 8.8 The rebuild record: synced before the build, removed by the swap or the failure batch, finished at open by dropping its prefixes the loaded schema does not declare; the simulator crashes and loses power at each rebuild stage

## 9. Vector index → `ultragraphdb_store_hnsw`

- [x] 9.1 Port `persistent_hnsw_test.go` (15), `kernels_test.go` (7), `ef_recall_test.go` (3); scenarios: vector-index
- [x] 9.2 Port the storage layout with its little-endian matrix, insert, delete, search with options (the empty-index result), the parameters from `exponent`, the transaction adapter over the local commit; the dead list in `ultragraphdb_store_apply`'s index maintenance
- [x] 9.3 Each Dgraph panic or fatal in the files of this group returns `HnswError`, and its test names the deviation
- [x] 9.4 A vector deleted and then set again leaves the dead list (ledger 53): the set removes its uid from the dead list in index maintenance; a scenario and a test naming the deviation from Go

## 11. Logging and telemetry → `ultragraphdb_common_telemetry`, every stage-1 and stage-2 crate

- [x] 11.1 `tracing`, `tracing-subscriber`, `opentelemetry`, `opentelemetry_sdk`, `opentelemetry-otlp` and `tracing-opentelemetry` in `[workspace.dependencies]`; `ultragraphdb_common/ultragraphdb_common_telemetry`: the config (level directive, plain or JSON, optional OTLP endpoint), `init` and its guard, `TelemetryError`; hermetic tests over a captured fmt layer and an in-memory span exporter
- [x] 11.2 `ultragraphdb_store_sim`'s capturing subscriber, without wall-clock time
- [x] 11.3 A `tracing` event at every place the ported Go code of stages 1 and 2 logs, at Dgraph's level, with its facts as fields; the schema store's skipped keys and failed reads among them; scenarios: telemetry

## 10. Close the stage

- [x] 10.1 Every scenario of this stage has its feature; `scripts/bdd_counts.sh` reports equal counts; `make test` green; `make fix` and `make format` clean
- [x] 10.2 Measure: batches of Dgraph-shaped commits at 14.5k and 50k commits/s with fjall's stall time per batch, rollups with the compaction filter over a large predicate, file sizes; record the results in the overview
- [x] 10.3 Attribution headers on every ported file, go-farm's MIT notice on `go_farm`; `AGENTS.md` crate and external-dependency tables updated
