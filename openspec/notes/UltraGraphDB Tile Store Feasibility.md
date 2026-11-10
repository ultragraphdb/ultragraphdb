# UltraGraphDB Tile Store: Feasibility Against the Source

Nov 10, 2026 · feasibility study of `UltraGraphDB Scalable Tile Store.md` (T1–T29) against the tree at `eb15d766` with
stage 6's uncommitted group-1 work, for the implementation plan of roadmap stage 6.5

The tile store can be built on this codebase with no `unsafe`, no repository macros and one new external crate
(`lz4_flex` with its safe codec, plus compio's `bytes` feature). It cannot be built as the design note states it. Five
blockers must be decided before any engine code is written, about forty design gaps must be specified, and stage 6's
storage groups must be re-sequenced, since they would otherwise build a third of UGS's storage on KV structures stage
6.5 deletes. Retiring the KV store removes about 47,000 lines outright and rewrites about 40,000 more.

The study read the code, the specifications and the Lean models; nothing was built or run. Five reviews covered the KV
surface and its consumers, the transaction and commit path, the runtime and compute reads, the catalog with UGS and
the indexes, and bytes, I/O and tooling. Several findings were reached independently by more than one review; those are
marked.

## 1. Blockers

### B1. The fold horizon depends on more than the log (found by three reviews)

The design folds versions into bases at the horizon `H`: the oldest pin, the oldest open read-write transaction and the
retention window (§4.8). Pins differ by replica, open transactions exist only on the primary, transactions end by the
clock (`partition_host_bound.rs:330-343`, `transaction/mod.rs:418-427`), retention runs on the wall clock
(`common_storage/.../retention.rs:41,50`), and replay runs with no pins at all. Compaction folding at that `H` writes
different tiles on every replica and on replay, which breaks T6, T21 and T29: healthy replicas would stop each other on
a checkpoint-identity mismatch.

*Handling.* Separate the fold point from the pins. The primary stamps a fold horizon into each Round, or it is derived
from logged epoch closes minus a retention counted in epochs; every replica folds at the logged value, and replay
folds where the first apply folded. Local pins only keep extra versions in memory and never change a written byte. The
folded horizon goes into the superblock, and a read below it answers TooOld, as `ts_too_old` does today. Digests must
not change when an entry is folded: versions stay outside the digested bytes. Decide before any compaction code; prove it
with two simulated replicas and a replay under different reads (an early E6).

### B2. The store is borrowed across turns (found by two reviews)

`Transaction<'a>` holds `applier: &'a`, as do `Execution<'a>`, `ParkedWrite<'a>`, `BoundTransaction<'a>` and
`ReadPin<'a>` (`transaction/mod.rs:63-82`, `partition_host/mod.rs:58-79`, `open_scan/mod.rs:45`), and apply mutates
through `&self` behind `Mutex`es (`changeset_core/mod.rs:58-71`), with `Send + Sync + 'static` on every generic. A
single-threaded engine without locks needs `&mut` for apply and cache fills while those borrows are open, which the
borrow checker refuses; `RefCell` would turn the conflict into runtime panics, which the no-panics rule forbids.

*Handling.* The core owns the engine. Transactions, Scans and parked work own their state (read version, logical write
overlay, read set, pending terms) and hold handles, never borrows; the core passes `&engine` or `&mut engine` into each
step; the `Send + Sync` bounds and the `S, C, E` type parameters (about twelve crates) go with the `Store` trait. This
rewrites the signatures of `transactions_txn`, `transactions_commit` and `cluster_partition` and must be the first wave,
since every later change sits on it.

### B3. Per-node settings shape the grid's bytes

Compaction triggers on the delta pool's watermark, sized from the node's memory budget (§4.9, §4.15 *Limits*); the
cleaner fires at 15% free of the grid file (§4.13); the checkpoint interval `C` is derived from the per-drive
power-loss flag (`wal_config/mod.rs`); lane L's bounds decide resume keys. Nodes with different memory, disks or drives
would compact differently and fail T29's comparison.

*Handling.* Every setting that decides a grid byte or a result is cluster-wide, identical and fixed: delta pool size and
watermark, `delta_row_entries_max`, θ, tile bounds, filter bits, extent size, grid capacity or a logical cleaner trigger,
`C`, `lane_entries_max`, `lane_bytes_max`. They go into a cluster-level settings table with a cross-check "equal across
storage instances" and the config service's `fixed` rule (`common_config_check/.../cross_checks.rs:734-775`), and into
the derivation version or the checkpoint identity. Cache size, I/O depths and `power_loss_protection` stay per node; the
last tunes round batching only.

### B4. T26's `Id128` keys conflict with UGS placement

T26 makes `Id128` the `@key` of `@multi` relations. In UGS a `@key` makes a relation node-encoded, places it at its key's
hash bucket and refuses `@colocate` (`SCHEMA.md` §11.1, `UGS0702`): every participant rule then reads another partition,
code 23 until stage 8, and the anchor's adjacency stops being local. `Id128`-keyed event entities scatter by hash too.
`Id128` also exists nowhere in `SCHEMA.md`, `CATALOG.md` or `PROTOCOL.md`.

*Handling.* Replace "`Id128` as `@key`" by an idempotency claim the relation's anchor partition holds, so the relation
stays edge-encoded and anchored; or accept the scattering. Either way add `Id128` (descriptor tag, datatype code, the
keyable list, fixed-width lowercase hexadecimal canonical form so text order is numeric order) before stage 6's groups
2–4 freeze golden bytes. T26 must be revised.

### B5. Stage 6 builds storage on KV structures stage 6.5 deletes

Stage 6 has built group 0 and tasks 1.1–1.2; `ultragraphdb_schema/` is empty and the compiled catalog of `CATALOG.md` is
specified only. Its groups 2b (the Ultra Style lift of `store_quads` and `store_apply`), 6.4–6.5 (claims and `@set`
combination keys in the dictionary and marks keyspaces), 7.5–7.7 (`@limit` aggregates, `asserted` as epoch quads,
revisions on KV MVCC, tombstones as statement records), 8.3–8.4 (index entries in `ValueIndex`/`CountIndex` posting
lists, materialized derivations as quads) and 9.2 implement roughly a third of stage 6 on structures the tile store
replaces outright (`openspec/changes/stage-6-ugs-data-definition/tasks.md`). Every finished group enlarges the port and
the differential test.

*Handling.* Decided by the user on 2026-11-10: stage 6 pauses once its task group 1 (the wire) is complete, stage 6.5
builds the tile store, and stage 6 resumes at group 2 on the tile store (section 6, order B, paused earlier).

## 2. Design gaps

Each gap is a place where the design note is silent or contradicts the code's obligations. The handling is the review's
recommendation; none is adopted.

### Execution and the commit path

| Gap | Evidence | Handling |
|---|---|---|
| **Tile misses on synchronous paths.** Every read during execution is synchronous: dictionary lookups, absence proofs, index maintenance, `references`, `Home::accept`, commit-time operations inside the pure step function, and apply's own reads (statistics duplicate checks, count-index degrees, `derive_referrers`). §4.9 builds a round in one turn; §3 admits an I/O on the commit path | `dictionary/mod.rs:253-267`; `partition_referrer.rs:44`; `home/mod.rs:81,103`; `changeset_core_referrers.rs:42` | Restartable attempts, not `await`: a read returns `Miss(tile refs)`, the request parks and reruns from its frame, as `ParkedWork::Turn` already does (`partition_core/mod.rs:119-128`); side effects (`applier.register` allocations) are undone first. Move base-reading derivations into execution, where they can yield, and log their results; apply then never reads a base |
| **The transaction's own writes.** Lane X reads must merge the transaction's uncommitted forward, reverse, index, count and delete changes; today `Request`/`MutableLayer` stack them | `execution/mod.rs:33-37`; `posting/.../request/mod.rs:50-60`; local-commit spec | A logical per-transaction overlay carried into lane X requests and merged within the request's budget; specify how reverse and index lookups see it |
| **Decided but unapplied rounds.** Round k+1 is decided while k syncs; commit-time operations read k's writes and earlier admissions through a `Batch` overlay | `commit_step/mod.rs:101-102`; `applier_operator_operator.rs:40-58` | A logical overlay of admitted proposals, alive until their round applies; operations read lane L plus it |
| **Pending IDs.** The pending map, `WriteTerms`, settle and release give every writer of an IRI one ID; `PENDING_MAX` = 65,536 refuses writes past it | `quad_store/mod.rs:25,43`; `dictionary/mod.rs:29-35,169-175` | Keep as engine-side state, without its `Mutex`es; size the bound from Ingest's limits; release on every abort, timeout and yield-cancel path, tested under the simulator |
| **The turn's shape.** Requests execute as they are taken; the turn waits for `fdatasync` inline and serves Scans after it; there is no slot for a compaction beat, a checkpoint step or one I/O submission | `partition_core_frames.rs:41-49`; `partition_host/mod.rs:283-285`; `turn_budget.rs:6-11` | Restructure: intake (parse only), commit batch, lanes with count budgets, beat, checkpoint step, I/O submission; split `Pipeline::sync` into issue and `Input::Synced`; decide whether the WAL takes appends while a sync is in flight |
| **Cross-core messages.** Unbounded `std::mpsc` polled on a 1 ms tick | `partition_mailbox.rs:26-31,66-68` | Bounded channels that wake the core, capacities fixed at startup |
| **Lane A against the specifications.** `index-maintenance` requires an insert to read the subject's forward list so a re-insert does not count twice; inserts of one new statement must conflict | `index-maintenance/spec.md:126-146`; `transactions/spec.md:58-80` | Amend the specifications for lane A's duplicate inserts and exact statistics, and extend `RoundModel` before relying on it |
| **The purge fence.** A purge's conflict range cannot abort a transaction whose stable reads were never recorded; purge has no record operation today | `run_check.rs:148-165` | One coarse stable-read marker per (tenant, type) in lane X, or a fence aborting every transaction older than the purge that read the type; add the purge operation |
| **Derived liveness.** A non-bitemporal relation is live only while its participants are; its visibility changes when a participant is retracted, so "stable reads record nothing" admits phantoms | `SCHEMA.md:1071,1111` | Lane L joins participant status; lane X records it for relations whose liveness is derived |

### State the design gives no home

| Gap | Evidence | Handling |
|---|---|---|
| **The session table,** written as versionless entries with each round and reloaded at open; Recall and exactly-once (T23) lose it once the WAL is trimmed | `sessions_store.rs:28-101`; `commit_step_round.rs:298` | A class M record run, checkpointed with the tiles |
| **The catalog,** stored in the partition's keyspace `0x01`, scanned by `Catalog::load`, recorded as a conflict range | `catalog_store.rs:78`; `reads.rs:68` | Load from the metadata group (`HeldCatalogs`); the catalog conflict becomes logical |
| **The dictionary's three kinds:** term→ID, ID→term, literal-hash→string, all under 128-bit hashes; §4.6 gives only the first, with a 64-bit hash | `dictionary_keys.rs`; note §4.6 | Specify all three as class I runs, keyed with the key bytes |
| **The applied position, the mapping-contract version** | `replay/mod.rs:128-130`; `quad_store/mod.rs:61-75` | Into the grid's superblock |
| **Two superblocks.** The WAL keeps a two-copy superblock with the checkpoint position; T22 puts a four-copy one in the grid with the same position. PROTOCOL §12.2's checkpoint paragraph describes fjall | `transactions_log/.../superblock/mod.rs:20-40`; `PROTOCOL.md:2700-2712` | The grid's superblock is authoritative; the order is grid durable, then the WAL's superblock, then the trim; rewrite §12.2's recovery rules |
| **Checkpoint and trim do not exist for partitions.** `trim_through` is called only by the sequencer and the metadata group; the store syncs only at stop; `UGDB_CHECKPOINT_BYTES` exists only in the backlog | `sequencer_snapshot.rs:47`; `partition_core/mod.rs:519` | New work, not a port: the scheduler, crash simulation at every step, E10 |

### Operations

| Gap | Evidence | Handling |
|---|---|---|
| **Drops** of a predicate, graph, tenant's data or tenant are prefix range deletes today, and a Drop of everything is how a tenant changes schema before stage 9.5; the tile header names no graph | `changeset_core_drop.rs:181-185,303-342`; `SCHEMA.md:2145` | Drop markers in the directory honoured by reads at or above the drop's version, deterministic removal in compaction, the graph in every relation identity, drop ranges logical in the WAL; state what snapshots below a drop see (today: nothing) |
| **Index builds** run on `ThreadExecutor` threads from a snapshot into temporary keyspaces, with rebuild records synced before the round | `changeset_core_rebuild.rs:95-240`; `changeset_applier/mod.rs:453` | Paced, deterministic build beats driven by log positions; the build record and touched set in the checkpoint; the start version pinned in the logged fold horizon. Governed tenants need none before 9.5; ungoverned tenants and stage 8's `@index(ann)` do |
| **The vector indexer** takes its own version under the apply lock, outside the log, over a timing-dependent queue | `changeset_core_vectors.rs:45-110` | Maintain HNSW at fixed log positions; until then it would break T29 |
| **HNSW against T14.** `store_hnsw` is Dgraph's persistent HNSW over `QuadKey`s, `Store` and `CachedList`; nothing reaches it at runtime before stage 8 | `vector_keys/mod.rs`; `traits_index/src/traits/txn.rs:34-50` | Retire it with the KV store and rebuild at stage 8 over adjacency blocks, keeping its search algorithm, distances and heaps: an exception T14 must state |
| **Deletes and `-> _` roles.** `cascade` is the default `@on_delete` and allowed on a `_` side, so a delete must find instances naming the participant; the design keeps no reverse rows for `_` roles and no predicate lists in the compiled layout | `SCHEMA.md:1146,713,1608`; note §4.7 | Keep local reverse rows for `_` roles; absence proofs enumerate the type's relations from the catalog; define the slot tombstone |

### Layout and catalog

| Gap | Evidence | Handling |
|---|---|---|
| **No layout record or version.** Replicas compiling the layout themselves diverge across binary upgrades | `CATALOG.md:528` (Report codes 902–906 only) | Emit the layout in compile phase 10 inside the catalog version; replicas read bytes and never compile. Decide before stage 6 group 4 pins golden bytes |
| **Graphs are missing from the compiled layout;** `SCHEMA.md` §10.2 lists a graph on the Type entry, `CATALOG.md` does not | `SCHEMA.md:821,590,1446`; `CATALOG.md:308-330` | A graph column or graph-split tiles; the graph in claim keys; align the two specifications |
| **`@bitemporal` revisions** are new versions of the same instance kept for all history, and every Gen5 type sits under `Statement @bitemporal`; a fixed-width column holds one value per slot | `SCHEMA.md:1087-1093`; `deepbrain-gen5.ugs:151` | Version chains per slot for revisable columns; a `?` role as a column only for relations that are not bitemporal |
| **T16 is underspecified:** "temporal types" is undefined; several candidate attributes, union participants and `@immutable Optional` values set after creation are not covered | `SCHEMA.md:684,1026` | Required attributes only, a common declaring supertype, a tie rule, a definition of "temporal" |
| **Shared predicates mix storage classes:** one predicate serves every type declaring the attribute, mutability 0 when they disagree | `CATALOG.md:399` | A class per entry, or index relations per (predicate, class) |
| **No sortable fixed-width value encoding.** The term encoding orders for equality only; negative integers sort after positive ones; `Interval<DateTime>` is 18 B, past the 16-byte inline cap | `encoded_term_cmp.rs:25`; `encoded_term_encode.rs:229` | One encoding per descriptor tag, enum ordinals included, before Ingest and group 8 |
| **Lossy and unbounded tokens.** Float tokens truncate to `i64`, `DateTime` buckets by hour, years truncate to `u16`; Gen5's `confidence @range(0,1) @index(range)` collapses into one or two hub rows; `exact` tokens have no length cap | `tokenizer_tokens.rs:52-56,171-189` | Order rows by the new encoding, consider exact tokens for floats, datetimes and measures, hash tokens past a bound |
| **`@set` combination key** is XXH3-64 of the participants; a collision returns the wrong instance | `SCHEMA.md:1785` | Key the run on the participant IDs, or verify on a hit |
| **Layouts change before 9.5:** Drop of everything and a new Install; ungoverned tenants' declarations and default entries at runtime; stage 8's `@index(ann)` under the same catalog version; "materialization across partitions" assigned to the tile stage | `PROTOCOL.md:2430-2470`; `SCHEMA.md:1881,1883` | Key the layout by (catalog version, layout format); online index relations for the generic layout; place cross-partition materialization in 6.5 or restage it |

### Reads and the protocol

| Gap | Evidence | Handling |
|---|---|---|
| **Scan order.** Kernel scans, compute's merge and resume, and conflict ranges assume `QuadKey` byte order; tiles order rows by slot. A conflict range over slots misses rows with later slots; a partial page covers a non-contiguous ID set | `kernel_scan.rs:15-17`; `conflict_history/mod.rs:16-40`; `transaction_scan.rs:29-113` | Define the scan order (a merge across slot runs in ID order, or slot order with whole-relation conflict ranges) and prototype both |
| **Logical conflict keys** are named but not specified; today they are physical list keys in WAL sections 6–8, index-token ranges included | `write_set_builder/mod.rs:24-38,248-282` | Families (forward, reverse, index token, count, statement, mark, claim) and an order in which a drop is one range; rewrite §12.2 sections 6–8; `core_keys` shrinks to this vocabulary |
| **Scan and Entries are KV-shaped;** frontier requests (type ranges, inline filters, top-k from the end, inline attributes) need a new command | PROTOCOL §13.12–13.14 | A frontier command in §13; Scan kept for the dictionary, referrer sets and statements and as the differential oracle |
| **Lane L against one version per Scan:** every Scan without a transaction is pinned to one version, and a frame naming another is refused | `partition_core_reads.rs:166-172`; `PROTOCOL.md:3128,3283` | Lane L answers one frame per turn with its version, or Scan `at` 2 maps to lane S; revise §13.13–13.14 |
| **Two resume rules:** Scan's `after` includes the object; T17 seeks strictly past the key | PROTOCOL §13.13 | One rule; strictly-past needs a (row, neighbour, statement) position |
| **Ingest framing.** A Round above 1 MiB is refused, so a 1 MiB share plus its resolution section cannot fit; §4.1's single align8 layout forbids a 4 KiB-aligned body; `OrderedLog::append` takes a borrowed body and `encode_round` copies it; receive buffers are unaligned with the body at +256 | `round_bounds.rs:27-33`; `core_body/src/utils/layout.rs:21-45`; `round_ref.rs:15-22`; `buffer_pool/mod.rs:35-50` | A `max_ingest` announced like `max_write_set`, cluster-wide; a Round section of Ingest bodies at 4 KiB frame offsets, a per-section alignment rule in `core_body`; `OrderedLog` taking owned scatter parts (its other implementors change too); aligned receive buffers read at offset 3,840. Ingest takes command 26; its reply lists shares; the session key gains the share; the Round's `derivation` becomes the layout version. With TLS, zero copy starts after decryption |

### I/O and tooling

| Gap or risk | Evidence | Handling |
|---|---|---|
| **Extent operations outside the ring.** compio has no fallocate or punch-hole operation, and a custom one needs `unsafe` | compio-driver 0.12.5; `compio_file_durable_file.rs:58-79` | rustix through `spawn_blocking`, as the WAL does; `EOPNOTSUPP` leaves space unreclaimed; T10 states the exception; allocate the grid at startup |
| **Tile cache frames,** each its own `AlignedBuf`, waste up to 4,095 B; a custom read buffer needs the `unsafe` `SetLen` | compio-buf 0.8.3 `io_buf.rs:369,728-735` | Enable compio's `bytes` feature; one aligned slab per size class split into `BytesMut` frames; a test proves alignment |
| **Cancelled I/O loses its buffer,** shrinking a fixed pool | compio's ownership model | Never cancel cache-fill or extent I/O; bound by count |
| **Unwritten extents:** the first `O_DIRECT` write into a fallocated extent costs a metadata change `fdatasync` journals | `compio_file_durable_file.rs:52-57` | E8 and E10 compare punch-hole reclamation against pre-zeroed extents |
| **macOS** has no `O_DIRECT` and no safe punch-hole; rustix offers safe `fcntl_nocache` and `fcntl_fullfsync` | rustix 1.1.6 `fs/fcntl_apple.rs:24,44` | macOS buffered and unreclaimed; every grid I/O change runs in a Linux container on ext4 or xfs |
| **Silent polling fallback:** compio drops to its polling driver on any io_uring setup error, flags included | compio-driver `fusion/mod.rs:26-41` | Set the flags in `core_runtime`, rebuild without them on a polling result, log the flags in effect |
| **No bit-packing, frame-of-reference or filter code,** and `std::simd` is nightly on toolchain 1.99.1; `#[target_feature]` calls need `unsafe` | `core_codec`; `Cargo.toml` rust-version | In-house, auto-vectorised over `chunks_exact`/`as_chunks`; E4 checks the generated code; raising the build's baseline target features is the user's decision |
| **Decoder lints** `indexing_slicing`, `arithmetic_side_effects`, `cast_possible_truncation`, `cast_sign_loss` are commented out of the one global table, which Ultra Style requires for decoders | root `Cargo.toml`; `ULTRA_STYLE.md:150-170` | The user's decision: enable them globally once the KV crates are gone, or keep decoders to review |
| **The partition on CPU 0** shares it with the main runtime | `run_service.rs:106,314` | Offset partition cores by one |
| **The derivation version** is a constant 1 and per-node startup parameters shape tiles | `pipeline/mod.rs:33-35` | Fold layout, compaction algorithm and B3's settings into it |

### Verification

- **`RoundModel`** depends on KV behaviour in three places: crashes keep an arbitrary applied prefix with checkpoints and
  trims unmodelled; `Store.read` serves any version where `H` folds; validation assumes full read sets. Add a
  checkpoint/trim step and `H`, re-prove `snapshot_stable` for versions at or above `H`, prove lane X's unrecorded stable
  reads and lane A sound, extend exactly-once to shares (`lean/RoundModel/Node.lean:66-70`, `Theorems.lean:36-55`). A small
  grid model (out-of-place writes, free-set reuse after N + 1, the superblock chain) is worth considering. `PlacementModel`
  depends on nothing of fjall.
- **The simulator.** The WAL's `SimStorage` is the base for the grid; its `FaultMode` gains lost acknowledged writes,
  misdirected writes and reads, latent sector errors, bit rot and failed punches. store_sim's generic parts (`Seed`,
  `SimRng`, `SimClock`, `LogCapture`, `Violation`, about 630 lines) move to a neutral crate; `tool_test`'s
  `PartitionWorkload` and `ChangesetWorkload`, with `INDEXES_MATCH_DATA`, `REVERSE_HOLDS_STATEMENTS`,
  `STATISTICS_MATCH_DATA` and `REPLAY_AGREES`, become the differential harness.
- **Golden vectors** for T20's digests from the reference C xxHash, as `generate_frames.c` does; new goldens for Ingest
  and the Round's Ingest section; a fuzz target for every tile decoder.

## 3. Corrections to the design note

Applied to the design note on 2026-11-10 with T30–T41.

- §5 says a partition takes one request per turn per connection. Commits already batch: up to 1,024 a turn, 36.6
  commits a sync in the baseline (`storage_session.rs:84-111`).
- T10's "all I/O through that core's ring" needs the extent-operation exception, and T14 the HNSW exception.
- T26 needs B4's revision.
- §4.17's zero copy starts after TLS decryption, and needs the Round and `OrderedLog` changes above.
- §4.7's "a role declared `-> _` keeps no reverse rows" contradicts cascade deletes.
- §4.8's class I includes `@bitemporal` content, which revisions contradict.
- §4.6's dictionary covers one of three kinds.
- §4.16's tile header must carry the graph.

## 4. What goes, what changes, what stays

| Disposition | Crates | Lines (src / tests) |
|---|---|---|
| **Deleted** | `ultragraphdb_traits_store`, `ultragraphdb_store_kv`, `ultragraphdb_store_posting`, `ultragraphdb_core_codec`, `ultragraphdb_core_algo` (unless compute keeps its sorted-set operations), store_sim's KV parts, `tool_test`'s KV parts | about 18,300 / 28,900 |
| **Rewritten or shrunk** | `ultragraphdb_store_quads` (about 3,200 lines of domain logic re-homed: the dictionary rules, pending map, `WriteTerms`, `Sequences`, skolem minting, referrer sets and marks, statistics with their sketch, `StatementRecord`, `DataDictionary`), `ultragraphdb_core_keys` (the logical-key vocabulary), `ultragraphdb_store_apply` (delta entries; builds and drops redone), `ultragraphdb_store_hnsw` (retired until stage 8), `tool_bench` (`wal_bench` ported, KV benches out); also `transactions_txn`, `transactions_commit`, `cluster_partition`, `common_storage`, the storage binary, `core_catalog`, `common_compute`, `protocol_wire` | about 25,300 / 36,400 counted, plus the uncounted crates |
| **Kept** | `ultragraphdb_transactions_log` and its `SimStorage`, the conflict history (byte-agnostic), `ultragraphdb_core_ids`, the tokenizers, store_sim's generic parts | — |

Specifications retired: `kv-store`, `posting-lists`, `posting-cache`, `uid-lists`. Rewritten: `key-encoding`,
`quad-storage`, `storage-simulation`, `term-dictionary`, `commit-rounds`, `local-commit`, `schema-changes`,
`vector-index`, `quad-reads`, `partitions`, `write-ahead-log`, `role-binaries`, `config-service`, PROTOCOL §12.1–12.2
and §13.12–13.14, `SCHEMA.md` §12 as logical obligations. store_apply's 19 BDD feature files (93 scenarios) are domain
behaviour and are retargeted, not deleted.

## 5. New crates

Each takes `[lints] workspace = true` and the repository's BUILD pattern; none needs an upward or sideways edge once the
durable seam is extracted.

| Crate | Group | Holds | Depends on |
|---|---|---|---|
| `ultragraphdb_error_tile` | error | `TileError(pub TileErrorEnum)`, raised by format, grid and engine | `ultragraphdb_error_invariant` |
| `ultragraphdb_traits_durable` | traits | `DurableStorage`, `DurableFile` (with `write_vectored_at`, `punch_hole`), `AlignedBuf`, moved out of `transactions_log` so the grid sits below the transactions layer | — |
| `ultragraphdb_store_tile_format` | store | Encodings, filters, header and sections, XXH3-64 checksums, XXH3-128 digests, LZ4; no I/O | `core_ids`, `error_tile`, `error_invariant`, `xxhash-rust`, `lz4_flex` |
| `ultragraphdb_store_grid` | store | Extents, streams, free set, manifest log, superblock, checkpoint, recovery, scrubbing, cleaner; sans-I/O core and compio shell | `tile_format`, `error_tile`, `traits_durable`, `compio` (`bytes`), `rustix`, `tracing` |
| `ultragraphdb_traits_engine` | traits | The engine's interface in logical change types, in place of `Store` | `core_ids`, `error_tile` |
| `ultragraphdb_store_tile` | store | Slots, deltas, versions and the logged horizon, lanes, compaction, digests, statistics records, the layout read from the catalog | `tile_format`, `store_grid`, `traits_engine`, `traits_catalog`, `core_catalog`, `core_ids`, `core_terms`, `core_mapping`, `error_tile`, `error_invariant`, `protocol_wire`, `tracing` |
| `ultragraphdb_store_tile_sim` | store | The engine under every fault class against a shadow model, replica byte-identity | `store_tile`, `store_grid`, `traits_durable`, `traits_engine`, `tracing` |

Two placements stay open: whether `CompioStorage` and `SimStorage` move with the seam or into a shared I/O crate, and
whether `traits_engine` names wire views (which would put a traits crate above `protocol_wire`) or only logical change
types.

## 6. Order of work

**Stage 6 and stage 6.5.** Two orders, each sound:

| | A. KV adapter first | B. Tiles before stage 6's storage groups |
|---|---|---|
| What happens | Stage 6 finishes behind a logical storage seam with a thin KV adapter; 6.5 replaces the adapter | After stage 6's storage-independent groups (1–5, 10, 11), 6.5 builds format, grid and engine on the generic layout, differential-tested against the KV store; groups 6–8 and 9.2 are then built on compiled-layout tiles directly |
| Thrown away | The adapter, and stage 6's KV storage code behind it | Nothing: governed tenants never run on the KV store |
| Risk | Lower; Gen5's acceptance on its present schedule | Gen5's acceptance waits for 6.5's engine, whose experiments have not run |
| By TigerStyle | Technical debt taken knowingly, behind a seam | "Do it right the first time": no double implementation |

The user chose B on 2026-11-10, pausing stage 6 earlier still: once its group 1 is complete. Groups 2 to 13 resume on
the tile store; group 2b, the Ultra Style lift of the KV commit path, is superseded by 6.5's ownership refactor.

**The order within 6.5, either way:**

1. **Decisions and specifications before code:** B1–B4; the cluster-wide settings table; the superblock's authority; the
   lint decision; the layout record inside the catalog version, graphs in the layout, `Id128`, the sortable value
   encoding (all before stage 6's group 4 pins golden bytes); logical conflict keys and the scan order; Ingest's framing;
   `SCHEMA.md` §12 and the stage-6 storage wording rewritten as logical obligations.
2. **The ownership refactor (B2)** with the logical seam, logical conflict keys, restartable reads and the two overlays,
   over the KV store, so every suite stays green and the differential test has its oracle.
3. **The durable seam extracted;** `tile_format` with its fuzz targets and golden digests; `store_grid` with the extended
   simulator and checkpoints; `RoundModel`'s checkpoint, horizon and lane extensions proved alongside.
4. **The engine on the generic layout,** differential-tested against the KV store through `tool_test`'s workloads;
   `wal_bench` ported through `core_runtime` with an engine switch; E1, E2, E3 and E6.
5. **The runtime:** the turn's new shape, the sync split, bounded channels, `core_runtime`'s flags with fallback, the
   discard thread, fjall's workers, `ThreadExecutor` and the `LeasedBlocks` mutex retired, checkpoint and trim, recovery
   from the superblock; E9 and E10.
6. **The protocol:** the frontier command, lane L's versioning, the resume rule, Ingest with its Round section; E12.
7. **The compiled layout** from the catalog, then (order B) stage 6's groups 6–8 and 9.2 on it; drops and builds as
   deterministic engine operations; the KV store, its adapter and `store_hnsw` removed (T14 with its HNSW exception);
   E7 and E14.

## 7. Decisions the plan needs from the user

1. B1: the fold horizon logged in the Round, or derived from logged epoch closes with retention in epochs.
2. B3: every grid-shaping setting cluster-wide, fixed and recorded in the derivation version or checkpoint identity.
3. B4: `Id128` as an idempotency claim at the anchor, or as `@key` with its scattering accepted; T26 revised either way.
4. B5: order A or order B for stage 6 and 6.5.
5. The grid's superblock authoritative over the WAL's.
6. The four decoder lints enabled globally once the KV crates are gone, or decoders left to review.
7. Raising the build's baseline target features for auto-vectorised decoding, or not.
8. `store_hnsw` retired at 6.5 and rebuilt at stage 8 (an exception to T14).
9. Cross-partition materialization of derivations in 6.5, or restaged.

## 8. Recommended resolutions

Each resolution below picks one answer where section 2 left options, argued from TigerStyle's principles: safety before
speed, explicit limits, determinism, the smallest surface, batching, and doing it right the first time. The user
approved all of them on 2026-11-10; the design note carries them as T30–T41, with T10, T14, T16 and T26 revised.

### The decisions of section 7

| # | Resolution | Why |
|---|---|---|
| 1 | **The primary stamps the fold horizon into every Round:** the minimum of its oldest open read-write transaction's version, its oldest pin, and a retention floor computed from the sequencer's logged close times (the latest epoch closed at least `retention_ms` before the round's own close). It never falls. Every replica folds exactly there; a read below the latest logged horizon answers TooOld. `retention_ms` is cluster-wide and at least the read-write bound (10 s) plus the longest read deadline, so a follower's snapshot read within the window is always answerable | The close times are already logged data, so the retention floor is computed, not measured; the primary is the only replica holding read-write transactions; one field in the Round replaces every local input to compaction |
| 2 | **Every grid-shaping setting is cluster-wide and fixed,** in a `[tiles]` table checked equal across storage instances and refused on change before stage 9.5; its digest joins the derivation version, which also names the layout format and the compaction algorithm | A per-node value that changes a byte turns a healthy replica into a fail-stop |
| 3 | **`Id128` is an idempotency claim, not a key:** an attribute of type `Id128` marked `@idempotency`, at most one per type, whose claim lives in the anchor's partition (an entity's own bucket, a relation's anchor); inserting an existing claim returns the existing instance, as `@set` does. `@key` keeps its meaning and placement. `Id128` joins the datatypes as a fixed-width lowercase hexadecimal canonical form | The relation stays edge-encoded and anchored, so its participant rules stay local; idempotency is what was missing, and it needs no new placement rule |
| 4 | Order B, pausing after group 1 | The user's decision |
| 5 | **The grid's superblock is authoritative.** It records the checkpoint's WAL position; the WAL's superblock records only its own segments. The crash order is: grid durable, grid superblock, WAL superblock, trim. Recovery reads the grid's superblock, then the WAL from its position | One source of truth per fact; the trim follows the only durable statement of what it may drop |
| 6 | **Enable the four decoder lints globally** when the KV crates are removed, at the end of 6.5; the tile crates are written to satisfy them from their first line, checked by review until then | Ultra Style requires them for decoders; zero warnings from the first line costs least when the code is new |
| 7 | **Keep the baseline target features;** E4 measures whether auto-vectorised decoding reaches its target on aarch64 and x86-64, and only a measured gap reopens it | The binaries ship multi-arch; a raised baseline excludes machines for a gain not yet shown |
| 8 | **Retire `store_hnsw` at 6.5 and rebuild it at stage 8** over adjacency blocks of slots, maintained at fixed log positions, keeping its search algorithm, distances and heaps | Nothing reaches it before stage 8, and its storage layer and indexer both contradict T6 |
| 9 | **Cross-partition materialization of derivations moves to stage 8,** which builds sequenced cross-partition work; materialization within the anchor partition stays in stage 6 | It needs cross-partition writes, which do not exist before stage 8 |

### The gaps of section 2 that offered options

| Gap | Resolution |
|---|---|
| Tile misses on synchronous paths | Restartable attempts: a read returns `Miss(tile refs)`, the attempt is undone and parked, the tiles load, the attempt reruns from its frame. Every derivation that reads a base moves into execution and is logged; apply reads deltas and in-memory state only, never a base |
| Lane A's duplicate inserts | Lane A skips read-set validation and keeps write-key admission against the conflict history, so two in-flight inserts of one new statement conflict at commit; execution proves the statement absent at its read version (filter, then the tile on maybe). Statistics stay exact; amend `index-maintenance` and `transactions` and prove it in `RoundModel` |
| The purge fence | A purge at version `v` aborts every open read-write transaction of its partition whose read version is below `v`. Purges are rare and coordinated; a fence needs no per-read marker |
| Derived liveness | Lane L joins participant status for relations whose liveness is derived; lane X records it |
| Drops | Drop markers in the directory, removed by deterministic compaction; reads at every version see the dropped data as absent, as today. A graph drop removes the graph's tiles from the manifest at once, since tiles are split by graph (below) |
| Index builds | None for governed tenants before 9.5. For the generic layout, deterministic build beats driven by log positions, their record in the checkpoint, their start version held in the logged fold horizon |
| `-> _` roles | Local reverse rows are kept; `-> _` hides the navigation name and joins no referrer set, nothing else |
| The layout record | Compiled once, inside the catalog version, as entries a replica decodes and never recompiles |
| Graphs | Part of every tile's identity: adjacency blocks per (graph, relation, direction), column chunks per (graph, type), claim keys with the graph. A graph drop is a manifest removal |
| `@bitemporal` revisions | Per-slot version runs, an adjacency row per slot whose entries are (asserted start, value), appended by each revision and never rewritten; `asserted_end` is the one class M column, written once. Revisions are appends, so class I holds; a `?` role is a column only for relations that are not bitemporal |
| T16 | Row order from a required `@immutable @index(range)` attribute declared by a common supertype of the participants; ties by the attribute's declaration order; "temporal" means a `Date`, `DateTime` or `Interval` attribute, newest first; otherwise neighbour ID order |
| Shared predicates across classes | Index relations and reverse rows split per (predicate, class), so a tile holds one class |
| Value encoding and tokens | One sortable fixed-width encoding per descriptor tag, specified in 6.5's first phase and used by Ingest, inline columns, row orders and range tokens; floats, datetimes and measures get exact tokens through it; tokens past 256 bytes are hashed and marked lossy |
| `@set` combinations | Keyed on the participants' IDs, never on their hash |
| Scan order | Forward rows scan in slot order within each bucket group; resume keys name (bucket group, slot, neighbour, statement); a partial lane X page records the whole (graph, relation, bucket group) as its conflict range, a completed scan its exact rows. Compute does not rely on key order across partitions. Measure the extra aborts in E5 |
| Logical conflict keys | Families: forward row, reverse row, index token, count, statement, mark, claim, session; ordered by tenant, graph, relation, so a drop is one range; PROTOCOL §12.2 sections 6 to 8 rewritten; `core_keys` keeps only this vocabulary |
| Lane L and Scan versions | A lane L request answers one frame per turn, each frame naming the version it read; §13.13 and §13.14 allow it for lane L only |
| Resume rule | Strictly past the last (row, neighbour, statement) answered, for Scan and frontier requests alike |
| Ingest framing | Command 26; a `max_ingest` derived from the round bound and the resolution's worst case, cluster-wide and announced; a Round section of Ingest bodies at 4 KiB frame offsets; `OrderedLog::append` taking owned scatter parts; receive buffers aligned so the body lands on 4 KiB; the share in the session key; `PENDING_MAX` sized from `max_ingest` |
| Extent operations | rustix through `spawn_blocking`, the exception written into T10; punch-hole by default, pre-zeroed extents if E8 and E10 favour them |
| The durable seam's implementations | `CompioStorage` and `SimStorage` move to a new `ultragraphdb_store_durable` below both `transactions_log` and `store_grid`; the traits stay in `ultragraphdb_traits_durable` |
| The engine trait | Names logical change types only, never wire views, so it stays at the traits layer |
| CPU 0 | Partition cores start at the second allowed CPU; the role's runtime keeps the first |

## Sources

Five read-only reviews of the tree on 2026-11-10: the KV surface and its consumers; the transaction and commit path; the
runtime, partitions and compute reads; the catalog, UGS and indexes with stage 6's change; bytes, I/O and tooling.
Dependencies read at the versions `Cargo.lock` pins: compio 0.19.2 (compio-driver 0.12.5, compio-fs 0.12.1,
compio-runtime 0.12.6, compio-buf 0.8.3), rustix 1.1.6, xxhash-rust 0.8.19, lz4_flex 0.13.1. Every `path:line` above is
relative to the repository or to that crate's source.
