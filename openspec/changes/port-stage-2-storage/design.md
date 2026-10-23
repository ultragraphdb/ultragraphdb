## Context

Stage 2 ports Dgraph's storage: `posting` (lists, rollups, the cache, index maintenance, rebuilds), `tok/hnsw`,
the storage half of `schema`, and the parts of `worker` that apply edges and schema changes to posting lists
(`worker/mutation.go`, `draft.go`'s `applyMutations`). It runs in one process: no Raft, no network, timestamps
from a local counter. Stage 1's crates supply keys, UID packs, values, tokens and parsed schemas. The source is the
Dgraph fork at `9fbf35614`: v25.5.0 plus six commits.

Dgraph runs Badger in managed mode (`worker/server_state.go:80-126`): versions are commit timestamps supplied by
the caller, no conflict detection, every version kept until compaction drops what a discard timestamp allows.
Above it, Dgraph keeps a second, in-memory version layer (`posting.Txn`, `LocalCache`, the oracle's pending
transactions and waiters), and the stale reads measured in scrith come from that layer and from the cache, not
from Badger (`openspec/notes/dgraph-port-overview.md`, §Storage).

## Goals / Non-Goals

**Goals:**

- A fjall store that provides every Badger capability Dgraph uses, behind one crate.
- Posting lists with Dgraph's read, merge, rollup and split behaviour.
- The single-batch commit: a request's writes go to the store as one batch at its commit timestamp, and nothing
  sits between "decided" and "applied".
- A cache that cannot return a list older than the store's.
- Index maintenance, schema changes, rebuilds, drops and the HNSW index, as Dgraph behaves.

**Non-Goals:**

- Commit decisions, conflict checks, timestamps from Zero: stage 3.
- The Raft log and replication: stage 4. Streaming snapshots to followers: stage 4.
- Backups and predicate moves, the Stream framework's other users: stage 5.
- The calculated-uid cache and uid warming (`committedUids`, `calculatedUids`, `warmCachedUids`, `mvcc.go:768-852`,
  `list.go:1757-1850`): a performance feature, left for later measurement.

## Decisions

### Crates

| Crate | Path | Depends on | Ports |
|---|---|---|---|
| `ultragraphdb_error_store` | `ultragraphdb_error/ultragraphdb_error_store` | — | Written fresh: `StoreError(pub StoreErrorEnum)`, raised by both `Store` implementations and carried by every crate above: a read below the discard timestamp (`Transaction is too old`, Dgraph's `ErrTsTooOld`), a failed read and a corrupted value, each naming its key, a failed batch, a poisoned store, a key over the size limit (Badger's `Key with size … exceeded … limit`) |
| `ultragraphdb_traits_store` | `ultragraphdb_traits/ultragraphdb_traits_store` | `ultragraphdb_error_store` | Written fresh: the `Store` trait and the types its signatures name (batch, persist mode, snapshot, scan), the `Executor` trait background work runs on, and the `ListCache` trait |
| `ultragraphdb_store_kv` | `ultragraphdb_store/ultragraphdb_store_kv` | `ultragraphdb_error_store`, `ultragraphdb_traits_store`; `fjall`, `moka` | The managed-mode Badger surface Dgraph uses, as the `Store` over fjall; the production `ListCache` over moka and the thread-backed `Executor` |
| `ultragraphdb_store_sim` | `ultragraphdb_store/ultragraphdb_store_sim` | `ultragraphdb_error_store`, `ultragraphdb_traits_store` | Written fresh: the simulated `Store`, its faults, the deterministic `ListCache`, the stepped `Executor`, the seeded workloads and their invariants |
| `ultragraphdb_store_posting` | `ultragraphdb_store/ultragraphdb_store_posting` | `ultragraphdb_error_store`, `ultragraphdb_traits_store`, `ultragraphdb_common_utils`, `ultragraphdb_core_algo`, `ultragraphdb_core_codec`, `ultragraphdb_core_keys`, `ultragraphdb_core_types`, `ultragraphdb_proto_pb` | `posting` (`list.go`, `mvcc.go`, `lists.go`, `writer.go`, `size.go`), the local commit; `PostingError` |
| `ultragraphdb_store_hnsw` | `ultragraphdb_store/ultragraphdb_store_hnsw` | `ultragraphdb_store_posting`, `ultragraphdb_core_keys`, `ultragraphdb_core_tok`, `ultragraphdb_core_types`, `ultragraphdb_common_utils`, `ultragraphdb_error_options`, `ultragraphdb_proto_pb`, `ultragraphdb_traits_index`, `ultragraphdb_traits_store`, `tracing` | `tok/hnsw`; `HnswError` |
| `ultragraphdb_store_apply` | `ultragraphdb_store/ultragraphdb_store_apply` | `ultragraphdb_store_posting`, `ultragraphdb_store_hnsw`, `ultragraphdb_core_keys`, `ultragraphdb_core_schema`, `ultragraphdb_core_tok`, `ultragraphdb_core_types`, `ultragraphdb_common_utils`, `ultragraphdb_error_num`, `ultragraphdb_error_store`, `ultragraphdb_proto_pb`, `ultragraphdb_traits_index`, `ultragraphdb_traits_store`, `tracing` | `posting/index.go` (index maintenance, rebuilds, drops), `worker/mutation.go`'s `runMutation` and schema changes, `draft.go`'s `applyMutations` without Raft; `ApplyError` |

The store crates sit in the `ultragraphdb_store/` group folder, which the root `Cargo.toml` takes as members with
`ultragraphdb_store/*`. Each crate above the trait runs its tests on both `Store` implementations.
`ultragraphdb_core_schema` gains loading and saving through the `Store` trait and depends on `ultragraphdb_traits_store`
alone for it. `ultragraphdb_common_utils` gains `go_farm`, an in-house port of go-farm's `Fingerprint64` (MIT, from
dgryski/go-farm) checked against golden vectors generated from Go, for posting identities and stage 3's conflict keys.

### Logging and telemetry

Dgraph logs through `glog` and survives many failures with a log line; the port logs the same events through
`tracing`, as structured events at Dgraph's level, and adds one wherever it survives a failure an operator needs to
see. Library crates only emit; `ultragraphdb_common_telemetry` is what a binary calls once to install the subscriber
(level filter, plain or JSON lines) and, when an OTLP endpoint is configured, the `tracing-opentelemetry` layer exporting
spans. Without a subscriber an event costs a check of a static flag, so the simulator stays deterministic; its own
subscriber records events in order without wall-clock time. Stage 3's binaries install it and add metrics.

### The store boundary and the storage simulator

Everything above the store reaches it through one synchronous trait, `Store` (`ultragraphdb_traits_store`), shaped in
batches: apply a write batch whose entries each name their key and version, with a persist mode; read a key at a
timestamp, read many keys at one timestamp in one call, scan a prefix or range of a snapshot; drop prefixes and drop
everything; set the discard timestamp and the tombstone horizon; compact; sync. `ultragraphdb_store_kv` implements it
over fjall; `ultragraphdb_store_sim` implements it in memory for the simulator. Stage 5 adds three operations to the
trait: sorted ingestion into a staged store, an atomic switch to it, and a size estimate per prefix; its tools reach
fjall only through `ultragraphdb_store_kv`. fjall 3.1 does its file I/O through `std::fs` and runs its own worker
threads, so a fault cannot be injected beneath it; the simulator injects faults at this boundary, which is where the
port's own storage logic meets the disk.

- **Faults.** A failed write batch, which poisons the store as it poisons fjall: the port stops applying, reports the
  error and recovers only by reopening, and the fault stream decides whether the failed batch is present, whole, or
  absent, whole, after recovery. A process crash, which may lose the batch being written; a power loss, which may lose
  a suffix, drawn by the fault stream, of the batches written after the last sync. Survival is prefix-closed in batch
  order, as fjall's journal recovery stops at the first torn entry. A crash or power loss during a drop. A read that
  fails, at once or partway through a scan, as fjall reports a block checksum failure. A stored value whose bytes are
  corrupted, which the posting-list decoder must refuse; on fjall a corruption on disk surfaces as a read error, so
  this fault tests the decoder. Slow operations, in simulated time.
- **Determinism.** A run is a function of its seed: the workload, the faults and the order in which the simulator steps
  the apply loop and its readers are drawn from separate streams derived from it, and time advances only when the
  simulator moves it. Storage code takes its clock, its randomness and its threads from what it is given. Background
  work (rollups, rebuilds, the parallel apply of `applyMutations`) runs on an `Executor` the caller passes, which the
  simulator steps. The posting cache sits behind the `ListCache` trait: in production it is moka, built with
  `build_with_hasher` and std's per-process `RandomState`, since cache keys hold
  client-chosen terms and a fixed hasher would let crafted keys collide; in the simulator it is a deterministic map with seeded eviction, since moka
  reads `Instant` on every access and evicts on wall-clock intervals. Outside `ultragraphdb_store_kv`, no storage code
  names fjall, moka, `Instant::now`, `SystemTime::now`, `thread::spawn` or an ambient random source. The workspace
  `clippy.toml` lists `Instant::now`, `Instant::elapsed`, `SystemTime::now`, `thread::spawn`, `thread::Builder::spawn`,
  `thread::scope` and `RandomState::new` under `disallowed-methods` beside the `mul_add` entries; a crate-local
  `clippy.toml` would replace the workspace file rather than add to it. The production implementations in
  `ultragraphdb_store_kv` and the existing uses in `ultragraphdb_core_algo`, `ultragraphdb_core_chunker` and
  `ultragraphdb_client`'s test utilities carry `#[allow(clippy::disallowed_methods)]` with their reason, and `make
  check` runs `cargo clippy --workspace --all-targets -- -D clippy::disallowed_methods`, which fails on any other use. A
  failing run prints its seed, and the seed replays it.
- **Workloads and invariants.** Each component adds its operations as it lands: batches, posting-list mutations,
  rollups and splits, index maintenance and rebuilds, drops, the discard timestamp. A shadow model holds what must be
  true, and after every step and every recovery the simulator checks it: a batch is visible whole or not at all, a
  synced batch survives any crash, the batches that survive are a prefix of batch order, the applied timestamp is the
  highest commit timestamp the store holds (rollup and drop batches leave it unchanged), a read returns only a value
  that was written at or below its timestamp, a dropped prefix stays dropped, no write lands below a deletion of the
  same key, and a failed batch is followed by recovery and nothing else.
- **Where it runs.** The simulator needs no file system, so a fixed set of seeds runs as an ordinary hermetic Bazel
  test tagged `unit-test` in `make test`; longer runs over fresh seeds are a binary run outside it.

The pattern is Apache Iggy's simulator (`core/simulator`), which runs the real server over in-memory doubles on a
seeded cooperative executor. Iggy simulates at the file level because it owns its file format; the port owns its
format at the store boundary, so that is where it simulates. Faults in fjall's own recovery are tested by killing the
process during a batch (task 1.6) and by stage 4's process kills.

### The store

- **Key encoding.** The stored key is the user key with every `0x00` byte written as `0x00 0xFF`, then the terminator
  `0x00 0x01`, then the bitwise complement of the version in big-endian order. fjall orders raw bytes and has no
  comparator, and Dgraph's keys are not prefix-free (an `exact` term `ab` is a byte prefix of `abc`), so Badger's
  `key ‖ !version` would interleave keys and their versions under fjall's order. The escaped, terminated form keeps
  Badger's order (`y.CompareKeys`: key first, then newest version first), keeps a key's versions contiguous, and maps a
  key prefix to its escaped prefix. "Newest at or below `t`" is one seek to the escaped key, the terminator and `!t`.
  Reads seek with a merging iterator rather than a point get, as `mvcc.go:858` does, because a rollup can write a
  version below versions already in higher levels.
- **Key size.** A write whose user key exceeds 65,000 bytes is refused, as Badger refuses it (`txn.go:352`), and so is
  one whose stored key exceeds 65,535 bytes, which escaping reaches first for a key dense in zero bytes: fjall's journal
  writes a key's length as an unchecked `u16`, and a longer key corrupts every batch after it.
- **Per-version metadata** in a value prefix: the meta byte, the discard flag, the tombstone flag.
- **Deletions and removals.** A *deletion* is a version with the tombstone flag: it hides its key's older versions for
  reads at and above its timestamp, and an incremental backup records it as a `delete`. The port writes deletions for
  schema and type deletes, for the split parts a rollup removes, and for the keys a rebuild's swap empties; Dgraph's
  edge and value deletes are delta postings, not deletions. A *removal* is an lsm-tree remove of one stored key, which
  hides that key at every timestamp; drops and the deletion sweep use removals.
- **Snapshots.** Every read of the fjall store, point or scan, goes through a `Database::snapshot()` covering both
  keyspaces, and a multi-key read uses one snapshot: fjall's keyspace reads run at `SeqNo::MAX` and see a batch's
  items while the memtable receives them.
- **One fjall keyspace for data,** plus one for store metadata, holding the store's drop records and the entries the
  caller names: in stage 2 the applied timestamp, the banned namespaces, the discard timestamp and the tombstone
  horizon; stage 4 adds the applied index and the applied proposals' keys. fjall's plain `Database` is used; its
  transactional databases are not, because the store never decides anything.
- **Garbage collection** is a compaction filter, under the port's rule. lsm-tree runs a filter only when a compaction
  merges tables, never on a flush, a trivial move or a table drop, so data no later merge touches is not filtered;
  `Store::compact` flushes the memtables and runs a major compaction (fjall's `#[doc(hidden)]`
  `rotate_memtable_and_wait` and `major_compact`, so fjall is pinned to 3.1.12), and tests and the deletion sweep use
  it. One filter is made per compaction and receives the compaction's items in key order (`lsm-tree`'s
  `CompactionFilter::filter_item(&mut self, ..)`), so it sees each key's versions newest first and drops those older
  than the newest discard-flagged or deletion version at or below the discard timestamp `D` (`Verdict::Remove` above the
  last level, `Verdict::Destroy` in it), Badger's rule for versions (`badger/levels.go:815-835`). The filter never drops
  a deletion: a key's older versions can sit in a table outside the compaction or in a higher level, and would return.
  A sweep reclaims deletions: at one snapshot, a deletion at `T` at or below the tombstone horizon, with no version of
  its key below `T`, is removed. Badger drops a deletion whenever no lower level overlaps the compaction
  (`levels.go:681, 829-835`), a check that cannot see a key's versions once the version is part of the key. The
  tombstone horizon is a parameter of the store: stage 2 sets it to the discard timestamp, and stage 5 sets it to the
  latest published backup's read timestamp, so the next incremental backup records the deletion. Backups learn of
  drops from the drop operations their manifest records (stage 5), not from deletions. The filter is total: an item it
  cannot decode is kept and counted, because a filter error or panic ends fjall's compaction worker and poisons the
  database. It reads `D` and the horizon from shared atomics, because fjall starts compactions inside `open`, before the
  store has read them from its metadata; until it has, `D` is 0 and the filter drops nothing. The filter is a trait
  object (`Box<dyn CompactionFilter>` from an `Arc<dyn Factory>`) because fjall's API requires one. The simulator's stepped executor holds its queued tasks as
  private `Box<dyn FnOnce() + Send>` values, the second exception: the tasks of different crates have different types,
  and the `Executor` trait itself stays generic.
- **fjall replays its active journal at every open,** flushed items included (`db.rs:700-735`), and rotates it only
  past 64 MB (`worker_pool.rs:192`): a version the filter destroyed would return at the next open. The filter therefore
  records each stored key it collects, and the store writes a journaled removal for each: at the end of `compact`, at the
  start of `sweep_deletions` in the same batch as the deletions' removals, and in a commit batch once 100,000 are
  pending. A collected version lies below a discard point at or below `D`, so no read the store serves returns it in the
  meantime.
- **The entries a prefix drop writes are fenced.** fjall's own garbage-collection watermark rises only at a memtable
  rotation with no snapshot open or every 10,000 snapshot closes, so versions a drop removed can still reach a later
  compaction under its removals, which the filter is never shown, and the filter would then collect the drop's own
  initial schema below them. A third flag bit, fenced (`0x04`), marks the entries a prefix drop writes; the filter never
  collects a fenced version. Drop-all needs no fence: `Keyspace::clear` leaves nothing behind. The cost is one small
  version per fenced entry staying on disk below `D`, where no read sees it, until a later drop of the prefix removes it.
- **No write lands below a deletion of the same key:** once the sweep has removed the deletion, such a write would be
  the newest version for reads above the deletion's timestamp. A rollup whose key gained a deletion above the
  rollup's version is discarded and redone.
- **The discard timestamp** never exceeds the oldest open read. Dgraph bounds it by the oldest pending transaction's
  start and by its change-data-capture timestamp (`draft.go:786`, `1135`) and not by read-only queries; change data
  capture is cut, so nothing else bounds it here. Until stage 4 ties it to snapshots, only tests advance it, and a
  stage-3 deployment keeps every version. fjall's key-value separation is not used, so there is no blob store to
  collect, where Dgraph runs Badger's value-log GC every minute (`x/x.go:1241-1257`).
- **Prefix drops are physical,** as Dgraph's `DropPrefix` is (`badger/levels.go:746-750`). A drop blocks commits to its
  prefixes, writes a drop record (its prefixes, its timestamp, and the entries to write once the prefixes are empty,
  such as a namespace's initial schema and types at timestamp 1) to the metadata keyspace and syncs it, removes every
  stored version under the prefixes with lsm-tree removals at one snapshot, in as many fjall batches as needed, writes
  the record's entries, syncs, deletes the record and unblocks commits; it then advances the cache's epoch. At open, a
  remaining record makes the store finish the drop before it serves. A read below the drop's timestamp finds nothing
  of the dropped data either, as in Dgraph. Dropping everything follows the same pattern around `Keyspace::clear`: a
  drop-all record, the clear, the root namespace's initial schema and types at timestamp 1, a sync, the record deleted;
  it also clears the banned set, the discard timestamp, the tombstone horizon and the applied timestamp.
  `Keyspace::clear` alone does not survive a power loss (it journals its marker at `PersistMode::Buffer` before it
  syncs the emptied tree), which the record makes safe. lsm-tree's `drop_range` is not used: it ignores sequence
  numbers and memtables, is not journaled, and fjall reaches it only through a hidden field.
- **Banned namespaces** are a persisted set; a batch drops the writes addressed to them and writes the rest, and reads
  filter them. Badger refuses such writes with `ErrBannedKey`, and Dgraph ignores the error (`draft.go:984`) after
  `CommitToDisk` returned before committing, which loses every write of that Badger transaction, the unbanned ones
  included (`writer.go:60-76`). Dropping everything clears the set; Dgraph's `DropAll` deletes the set on disk and
  keeps it in memory until a restart (`badger/db.go:414-428`).
- **Durability.** Every batch is written with at least `PersistMode::Buffer`, which hands the whole batch to the
  operating system; without a persist mode, part of it can stay in fjall's 8 KiB buffer. Until a Raft log exists
  (stages 2 and 3), the store carries durability with Dgraph's two modes: `process` hands each commit batch to the
  operating system and syncs once a minute, `filesystem` syncs a commit batch before it is acknowledged. From stage 4
  the Raft log carries durability, and the store is rebuilt from a snapshot and the log's entries after its applied
  index. A write is durable once a sync has covered it. After a crash the store's applied timestamp names the last
  commit batch it holds. A failed batch poisons fjall: every later write, persist and clear returns `Poisoned` until the
  database is reopened, and a batch whose persist failed is absent in this process and may be replayed from the journal
  at reopening. The port stops applying, reports the error and recovers only by reopening; a failed compaction poisons
  the store the same way.
- **What corruption the port detects.** On fjall, a corrupted block fails its checksum and surfaces as a failed read,
  which the port reports. The simulator's corrupted-value fault garbles a value's bytes to test the decoders above the
  store; they refuse a value whose structure no longer decodes, and a flip that leaves a valid encoding (a protobuf name
  byte turned into another valid byte) is beyond any decoder, so the simulator's workloads draw only the corruptions a
  checksum-free decoder can see.
- **Settings compaction acts on are synced when set.** The discard timestamp, the tombstone horizon and a banned
  namespace are durable when the call that sets them returns: compaction collects by them, so a setting lost to a power
  loss after compaction used it would let reads between the lost and the used value return collected data silently.

Alternative: redb. Decided against on 2026-10-20 (`dgraph-port-overview.md`).

### The commit

- A request owns a scratch layer per touched list: Dgraph's `MutableLayer.currentEntries`. Applying an edge reads the
  list with the request's earlier edges on top of the state at its read timestamp, and index, reverse and count
  maintenance read and write through it. Scratch layers stack: a request applied after another in one batch reads
  that request's writes, as stage 3's batches and upsert queries need. A scratch layer and the layers stacked under it
  are seen at the request's own read timestamp only; Dgraph's `iterate` also shows them to reads at later timestamps,
  while its `findPosting` and `listLen` do not.
- The writer's `commit_with` hook runs under the writer lock before commit timestamps are assigned: the commit-time
  schema conversion and the refusal of requests that read before a drop or schema change run there. A request the hook
  refuses fails together with every later request of the batch whose writes stack on its own.
- Dgraph's read strategy is ported with two completions. A list built without a store read (`GetFromDelta`,
  `lists.go:257-296`) is completed from the store before any read of it in the same request, and `GetScalarList`
  (`oracle.go:158-185`) reads through the merging iterator rather than a point get, so index maintenance always finds
  the old value.
- A request's edges read a schema overlay local to the request, holding the predicates its edges create under Dgraph's
  creation rules (`worker/mutation.go:300-327`): the type inferred from the value, a new uid predicate as `[uid]`, and a
  `SINGLE` or `LIST` hint overriding the list flag. At commit, under the writer lock, a request whose new predicate
  gained a schema since its read converts its values to that schema, its list flag and the posting identities that
  depend on it included, as Dgraph's apply loop converts a later proposal's edges (`mutation.go:73-77`), or fails with
  Dgraph's conversion error. The first commit's schema wins.
- Commit marshals each touched list's scratch layer as a delta and writes all of them, the index, reverse and
  count deltas, any schema entries and the applied commit timestamp, as part of one fjall batch at the commit
  timestamp, then updates the cache under the same writer lock. A batch may hold several requests' commits, each at
  its own timestamp: stage 3 writes an apply batch as one store batch, and stage 4 writes a snapshot's keys with their
  many versions.
- A request takes its commit timestamp from the writer when its batch is written, so every assigned timestamp is
  applied, and a request that fails before then holds none.
- A request whose read timestamp is below the timestamp of a drop or schema change, and that writes a key under that
  drop or predicate, fails at commit with `Transaction has been aborted. Please retry` and writes nothing. Dgraph aborts
  its pending transactions on namespace and full drops and refuses a predicate drop or schema update while pending
  transactions touch the predicate (`draft.go:315-333, 343, 379, 392, 440, 476`); the port has no pending transactions,
  so the request fails instead of the operation.
- Not ported: `pendingTxns`, the oracle's waiters, `ProcessDelta`, `shouldAbort`, `ResetTxns*`,
  `detectPendingTxns`, the `deltas` and `maxVersions` maps, `FillContext`. Conflict keys (`GetConflictKey`) are
  ported in stage 3 with the commit decision, where they belong.
- One writer per store applies commits in order. The applied timestamp advances after each batch and its cache
  update, and a read at `t` waits until every commit timestamp assigned at or below `t` is applied.

### The cache

The cache sits behind the `ListCache` trait, keyed by the list's key, each entry holding an `Arc` of the list, the
commit timestamp it reflects and the cache's epoch when it was written. A commit replaces or removes the entries of
the lists it wrote before the writer lock is released. A reader at `t` uses an entry only if the entry's commit
timestamp is at or below `t`, no commit to that key is applied in (the entry's timestamp, `t`], and its epoch is
current; the cache holds the newest list, so the second condition holds whenever `t` is at or above the applied
timestamp at the time the entry was written. A reader that missed inserts its list only if no commit to that key was
applied after its snapshot and the epoch is unchanged, checked under the writer lock. A drop, a schema change and a
rebuild advance the epoch under the writer lock, which makes every older entry a miss and every older fill a refused
one; moka's `invalidate_all` keeps an entry filled in the same clock tick as the clear (`base_cache.rs:2598-2604`).
Dgraph refuses a fill older than the entry it would replace (ristretto's `ShouldUpdate`, `mvcc.go:532-534`). It can
still fill an absent entry with a pre-commit list, which the #9597 check keeps from being served. The port refuses that
fill too, so the entry can serve reads above the list's newest version. Readers above a list's newest version, the
common case, therefore hit; in Dgraph they miss, because a hit requires `readTs ≤ maxTs` of that list
(`mvcc.go:776-781`, the fix for issue #9597).

### Rollups

Rollups run in batches on the `Executor`, as Dgraph's `incrRollupi` does (priority for lists with more than 500
deltas): a rollup takes a fresh timestamp, waits until every commit below it is applied, rolls up there, and writes its
main key, its parts and the deletions of the parts it removes as one batch at the main key's version, through the same
writer. Dgraph writes the parts one version below the main key (`list.go:1455-1458`, `1539`), and its `TxnWriter`
commits each key in a Badger transaction of its own (`writer.go:47-76`), so a crash can keep a main list whose parts
were never written; it also splits a large commit across transactions on `ErrTxnTooBig` (`mvcc.go:286-318`). In the
port a rollup and a commit are each one atomic batch. A rollup leaves the versions below it readable, so open reads
bound only the discard timestamp. Rollups pause while an index rebuild runs. A rollup whose key a drop covered after
the rollup's read is discarded, checked under the writer lock, so a rollup read before a drop cannot write dropped
data back after it; Dgraph's drops start no task that stops rollups (`draft.go:116-197`). The check is the cache's
epoch, which every drop, schema change and rebuild advances under the writer lock, so a rollup is discarded on any of
them, and also when its main key gained a deletion above the version it read. A list without deltas is not rewritten
(Dgraph rewrites it at `minTs + 1`). Rollup runs are serialised, as Dgraph's one rollup goroutine is.

- **A split list's parts are read with its main key, through one snapshot.** A lazy read of a part would need a
  snapshot at the list's `minTs`, which is refused below the discard timestamp, and a cached main key combined with
  parts from a newer snapshot breaks once a rollup rewrites them.
- **The local counter starts at the applied timestamp plus two after an open.** A rollup writes at the newest commit
  plus one, which can be the applied timestamp plus one; a counter restarting there would write a delta over that
  complete version.
- **A cache entry's timestamp is the applied timestamp at the miss,** not the list's newest version, because the store
  does not report deletions; a fill is made with `try_lock` on the writer, so it never waits for a commit and a read
  inside a commit hook cannot deadlock.

### Rebuilds and drops

Ported as Dgraph has them: the `needs*Rebuild` decision tables, the dual schema (`mutSchema` for writes, the
interim query schema of `GetQuerySchema` for reads, `index.go:1132-1160`), the two-phase rebuild through a temporary
store (vector indexes rebuild in place, as `RunWithoutTemp` does, reading each key's merged list where Dgraph reads its
newest version alone, `index.go:720, 752`), a `@count` rebuild's rewrite of every data list of the predicate as a
complete version with the discard flag (`index.go:987-988`, `1083-1103`), drops of a vector predicate with its
`__vector_entry`, `__vector_` and `__vector_dead` predicates (`schema.go:464-479`), and type changes with their
refusals. These parts differ:

- A failed rebuild keeps the previous schema's indexes complete. Dgraph drops indexes and rewrites list values before
  the build and restores only the schema record on failure (`worker/mutation.go:120-129`, `262-275`), and its writes
  during a rebuild maintain only the new schema's indexes (`mutation.go:59`, `schema.go:332-345`). The port builds into
  the temporary store first, and while it builds, writes maintain the previous schema's indexes as well as the new
  schema's. When the build succeeds, one atomic batch swaps them: for every key under a rebuilt prefix, the built
  complete list, or a deletion where the build produced none, at the rebuild's timestamp; for every key under a removed
  prefix, a deletion at the batch's own commit timestamp, above every write that maintained it; the list-type rewrite
  (`BuildData`) and a `@count` rebuild's data lists; and the new schema record. A value-type change deletes
  every old tokenizer's prefix and rebuilds every current one (`index.go:1255-1265`), and the swap covers both. A failed build writes,
  in one batch, a deletion for every key under a rebuilt prefix the previous schema does not declare.
- A type change is checked before anything changes, and a refused change alters nothing. Dgraph refuses a list-to-scalar
  change on an empty predicate in `BuildData`, after `DropIndexes` has run (`index.go:1795-1812`,
  `mutation.go:255-268`). A predicate holds data when one of its data lists is non-empty at the change's timestamp;
  Dgraph's `hasEdges` counts any key whose newest version is not empty, by its own note an approximation
  (`mutation.go:355-376`).
- Drops remove split parts too; Dgraph's `DeletePredicate` and `DeleteData` drop only the default prefix and leave
  split parts behind (`index.go:1881`, `1894`). Dropping everything in a namespace also removes its schema and type
  keys, which Dgraph removes in memory only (`schema.go:150-168`).
- Three builds run while commits are held back rather than in the background: reverse edges on a predicate with
  `@count` in the old or the new schema, a list-type rewrite, and every vector build. Writes beside them would compute
  counts from reverse lists the build has not yet written; Dgraph has the same race (`index.go:1200-1217`,
  `1759-1793`), and the simulator found it in the port (ledger 54). Every other build runs in the background as above.
- A vector rebuild is one request at the change's timestamp: it deletes the HNSW predicates' lists and inserts every
  vector, so a failed build writes nothing and the previous index stays.

### The vector index

Ported from `tok/hnsw` and stored as Dgraph stores it (the vector-index spec). Its in-memory caches `nodeAllEdges` and
`deadNodes` live for one insert or one search, as in Dgraph: every insert and search builds a fresh `persistentHNSW`
(`tok/index_factory.go:56-60`, `tok/hnsw/persistent_factory.go:97-169`). The dead list itself is written by index
maintenance (`posting/index.go:113-152`), so it belongs to `ultragraphdb_store_apply`. `VectorIndex::insert` takes the
source of its level draws, a uniform value in [0, 1) from a seeded source the caller supplies, so a seed replays the
graph. `HnswFactory` keeps no map of indexes, since every insert and search builds its own, so its `find` returns none.

The stage-1 traits `VectorIndex`, `IndexFactory`, `OptionalIndexSupport` and `OptionalSearchOptions` are reshaped so
`PersistentHnsw` implements them: each method generic over a cache is bounded `C: CacheType<Error = Self::Error>`, and
`insert` takes the transaction it writes through, as Go's insert writes only through a `*TxnCache`
(`persistent_hnsw.go:444-448`). An HNSW option below 1 (`maxLevels`, `efConstruction`, `efSearch`, `exponent: "0"`) is
refused when the schema change carrying it arrives, so no stored schema holds one; Go accepts it and panics on search.
The index's build keeps the same check.

## Risks / Trade-offs

- [fjall runs the compaction filter only when a compaction merges tables] → Versions spread across tables and levels
  are collected late, and data no later merge touches waits for `compact`. Measured on the workloads in the overview's
  "What to measure early".
- [One writer per store] → The commit rate is bounded by one thread's batch rate. Dgraph's apply path is serial
  per group already; the writer batches commits from one apply step. fjall's write stalls run in the committing
  thread while the writer lock is held: a spin at 20 or more level-0 runs and 100 ms sleeps at 4 or more sealed
  memtables (`batch/mod.rs:176-181`, `keyspace/mod.rs:802-822`), so the applied timestamp, and every read waiting on it,
  waits too, and compaction bounds the commit rate as well. Task 10.2 measures stall time per batch.
- [Updating cached lists in place] → Dgraph mutates shared maps unless `refresh=true`. Lists in the cache are
  immutable `Arc`s, replaced on write.

## Migration Plan

New crates; stage 1's `ultragraphdb_core_schema` gains a loader over the `Store` trait. Go-written Badger directories are
not read; data moves through export and the loaders (stage 5).

## Open Questions

- Does the compaction filter alone keep version garbage bounded under Dgraph's rollup rate, or is a background
  sweep over hot keys needed as well?
- Which store holds a rebuild's temporary entries: memory with spill to disk, or a fjall keyspace from the start?
