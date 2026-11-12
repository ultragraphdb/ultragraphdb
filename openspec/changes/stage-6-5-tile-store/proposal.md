## Why

Every partition keeps its graph in fjall, a general key-value store: one row, one list, one fjall block, a version
suffix per key, and a write path that syncs fjall's own journal beside the log. The benchmark of
`openspec/benches/benches-2026-11-09.md` and the measurements of the design note show where that ends: a cold Scan
spends 89% of its time loading and checksumming a fjall block per list, MVCC copies are paid for data that never
changes, and two replicas of one partition hold different bytes, so repair and verification compare logical contents
instead of checksums. Stage 6.6's storage groups would build a third of UGS's storage on those structures, which is why
the order of work decided on 2026-11-10 pauses stage 6.6 after its group 1 and builds the tile store first.

The tile store is specified to the byte: `specification/storage/STORAGE.md`, its conformance vectors under
`specification/conformance/storage/`, and the protocol and schema sections it reads and writes, all made consistent by
the review of `openspec/notes/UltraGraphDB Spec Consistency Review.md` (T48). The code does not yet follow any of it.
This change builds the engine those documents describe and moves every crate that touches storage, the log or the
wire's read and write kernels onto it, so that code and specification say the same thing.

## Success

1. **Every suite green on the tile store alone,** with the KV store, its traits and its posting lists deleted.
2. **The conformance vectors reproduced.** The format, grid and engine crates encode every vector of
   `specification/conformance/storage/` byte for byte, decode it back, refuse every refused vector with its refusal, and
   the engine fed the reference engine's rounds reproduces its rewrites, checkpoints, final grid and crash recovery.
3. **Determinism under faults.** Two simulated replicas of one partition fed the same rounds hold the same tiles at the
   same addresses and the same checkpoint identity under every fault class of `STORAGE.md` section 19.1.
4. **Differential equality before removal.** Every workload of `tool_test` answers the same on the tile store as on
   the KV store, before the KV store is deleted.
5. **The Lean model amended and proved:** checkpoints, the logged fold horizon and the lanes in `RoundModel`.
6. **The net impact measured.** The benchmark of `benches-2026-11-09.md` rerun on the tile store exactly as it ran on
   the KV engine, recorded as a dated baseline beside it, with a table comparing every figure of the two runs; it runs
   last, after the tokenizers and the KV store's removal.

## What Changes

- **The engine.** New crates for the tile format, the grid and the engine (design D1): tiles of three shapes, the
  free set, the manifest, the superblock, checkpoints, recovery, repair, scrubbing and the cleaner; slots, the delta,
  versions and the logged fold horizon; apply, admission and deterministic compaction; drops, purge markers and index
  builds as logged engine operations; the generic layout every tenant has before UGS resumes. The compiled layout of
  governed tenants is stage 6.6's (groups 3, 4 and 6 to 8, 9.2): this change reads a catalog version's Layout entry only
  through the interface those groups fill.
- **Ownership.** The engine is owned by its core; transactions, Scans and Frontier requests hold handles and restart
  their attempt when a tile is not in the cache (T33). The commit path writes logical conflict keys (`PROTOCOL.md`
  section 12.2) and keeps two overlays, a transaction's own writes and admitted proposals. This supersedes stage 6.6's
  group 2b.
- **BREAKING — the log's record.** The Round carries what execution derived: `flags` with the checkpoint bit, a
  derivations section (count degrees, `@limit` and statistics deltas) and an evictions section; Ingest shares log
  their body as received with a resolution and its skipped list. The session table is keyed by session alone. The
  derivation version becomes a u64 over the `[tiles]` settings. Logs written before this change are not readable; there
  are no outside users.
- **BREAKING — the read kernels.** Scan reads records by logical key and families whole; Frontier (command 532) and
  Neighbours (533) read rows by logical key in lanes L, S and X, with kinds 1 to 6, every-predicate requests and inline
  filters; resume keys are logical and carry no slot; results come partition by partition. Retention is the logged fold
  horizon alone: `read_retain_ms` and the discard version go.
- **BREAKING — Ingest,** command 26: block-columnar inserts split into atomic shares per partition, with key and
  `@idempotency` claims and blank-node groups placed in one share.
- **BREAKING — the Welcome** announces `value_bytes_max`; error codes 5 and 6 carry the store's admission and value
  refusals.
- **Tokenizers without Dgraph's legacy.** Range, interval and count tokens are the tokenizer's identifier byte and
  the value's comparison form, exact; the `datetime` tokenizer replaces `hour` and `day`; identifier bytes are the
  registry's own; tokens are no longer required to equal Dgraph's.
- **The runtime.** A partition's turn takes intake, the commit batch, the lanes, one compaction beat, one checkpoint
  step and one I/O submission; the sync splits into issue and completion; fjall's workers, the discard thread and the
  thread executor retire.
- **Removed:** `ultragraphdb_store_kv`, `ultragraphdb_traits_store`, `ultragraphdb_store_posting`,
  `ultragraphdb_core_codec` and `ultragraphdb_store_hnsw` (rebuilt on tiles in stage 8), with their specifications.

## Capabilities

### New Capabilities

- `tile-format`: the bytes of tiles, records, encodings, filters, digests and references (`STORAGE.md` sections 1 and
  4 to 11) and their decoders' refusals, held to the conformance vectors.
- `tile-grid`: the settings of section 3.1 and the derivation version (section 3.2), the grid file, extents and
  streams, allocation and the free set, the manifest and directory, the superblock, placement order, checkpoints,
  recovery, state sync, repair, scrubbing and the cleaner (sections 3.1, 3.2, 12, 13.1 to 13.3, 16.5, 16.6 and 17).
- `tile-engine`: slots, the delta, versions and the fold horizon, storage classes, apply, admission, compaction, the
  generic layout, drops, purges and builds (sections 13.4, 14, 15, 16.1 to 16.4, 16.7 and 16.8); placement order is
  `tile-grid`'s.
- `storage-lanes`: lanes L, S and X, what a read sees, Scan and Frontier served from tiles, logical resume keys, stable
  reads and what lane X records (section 18; `PROTOCOL.md` sections 13.13, 13.14, 13.21 and 13.22).
- `ingest`: command 26 end to end, from the frontend's split to the partition's share and its Round section
  (`PROTOCOL.md` section 6.8).
- `tile-simulation`: the engine under every fault class against a shadow model, replica byte-identity, and the
  reference engine's run reproduced (sections 19 and 22).

### Modified Capabilities

- `commit-rounds`: the Round's flags, derivations, evictions and Ingest sections; execution derives and apply adds.
- `local-commit`: logical conflict keys, the two overlays, restartable attempts and the ownership of the engine.
- `transactions`: what lane X records, the fold horizon as the only retention, TooOld below it.
- `quad-reads`: Scan and Frontier as the read kernels, the partition-by-partition order, logical resume keys,
  `read_retain_ms` removed.
- `quad-storage`: quads kept in tiles' families in place of keyspaces.
- `key-encoding`: logical keys only, conflict keys and record keys; the KV key layouts removed.
- `term-dictionary`: record kinds 1 to 3, term-to-ID in class M, the replaced ID, copies outside bucket groups.
- `partitions`: the turn's shape, the session table keyed by session, logged evictions, admission refusals.
- `write-ahead-log`: checkpoints as `STORAGE.md` section 17 states them, the trim, the WAL superblock.
- `storage-simulation`: the grid's fault classes and the replica-identity check.
- `schema-changes`: drops, builds and purges as deterministic engine operations.
- `role-binaries`: the storage role's runtime and its retired threads.
- `wire-protocol`: Welcome, Scan, Entries, Frontier, Neighbours, Ingest and error details as `PROTOCOL.md` states them.
- `tokenizers`: exact range tokens, the `datetime` tokenizer, the registry's identifier bytes, no Dgraph equality.
- `vector-index`: retired until stage 8.
- `kv-store`, `posting-lists`, `posting-cache`, `uid-lists`: removed.

## Impact

- **New crates:** `ultragraphdb_error_tile`, `ultragraphdb_traits_durable`, `ultragraphdb_traits_engine`,
  `ultragraphdb_store_tile_format`, `ultragraphdb_store_grid`, `ultragraphdb_store_tile`, `ultragraphdb_store_tile_sim`.
- **Rewritten or shrunk:** `ultragraphdb_store_quads`, `ultragraphdb_core_keys`, `ultragraphdb_store_apply`,
  `ultragraphdb_store_sim`, `ultragraphdb_transactions_txn`, `ultragraphdb_transactions_commit`,
  `ultragraphdb_transactions_log` (its durable seam moved out), `ultragraphdb_cluster_partition`,
  `ultragraphdb_common_storage`, `ultragraphdb_common_compute`, `ultragraphdb_common_frontend`,
  `ultragraphdb_protocol_wire`, `ultragraphdb_core_catalog`, `ultragraphdb_core_tok`, `ultragraphdb_tool_test`,
  `ultragraphdb_tool_bench`, the storage, compute and frontend binaries and their test crates, the clients' wire types.
- **Deleted:** `ultragraphdb_store_kv`, `ultragraphdb_traits_store`, `ultragraphdb_store_posting`,
  `ultragraphdb_core_codec`, `ultragraphdb_store_hnsw`; fjall and moka leave the workspace; `ultragraphdb_core_algo`
  unless compute keeps its sorted-set operations.
- **Golden frames and fuzz targets** of every changed body regenerated; the Lean `RoundModel` extended.
- **Stage 6.6** resumes at group 2 on this change's engine; its group 2b is superseded, and its tokenizers delta follows
  this change's.
