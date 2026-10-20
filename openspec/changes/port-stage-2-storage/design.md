## Context

Stage 2 ports Dgraph's storage: `posting` (lists, rollups, the cache, index maintenance, rebuilds), `tok/hnsw`,
the storage half of `schema`, and the parts of `worker` that apply edges and schema changes to posting lists
(`worker/mutation.go`, `draft.go`'s `applyMutations`). It runs in one process: no Raft, no network, timestamps
from a local counter. Stage 1's crates supply keys, UID packs, values, tokens and parsed schemas.

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

## Decisions

### Crates

| Crate | Path | Ports |
|---|---|---|
| `store_kv` | `ugdb_store/store_kv` | The managed-mode Badger surface Dgraph uses, over fjall |
| `store_posting` | `ugdb_store/store_posting` | `posting` (`list.go`, `mvcc.go`, `lists.go`, `index.go`, `writer.go`, `size.go`), the local commit |
| `store_hnsw` | `ugdb_store/store_hnsw` | `tok/hnsw` |
| `store_apply` | `ugdb_store/store_apply` | `worker/mutation.go`'s `runMutation` and schema changes, `draft.go`'s `applyMutations` without Raft |

`core_schema` gains loading and saving through `store_kv`.

### The store

- **Key encoding.** `user key ‖ !version` (big-endian), as Badger encodes it, so a key's versions sort newest first
  and "newest at or below `t`" is one seek to `key ‖ !t`. Reads seek with a merging iterator rather than a point get,
  as `mvcc.go:858` does, because a rollup can write a version below versions already in higher levels.
- **Per-version metadata** in a value prefix: the meta byte, the discard flag, the tombstone flag.
- **One fjall keyspace for data,** plus one for store metadata (applied timestamp, banned namespaces, discard
  timestamp). fjall's plain `Database` is used; its transactional databases are not, because the store never
  decides anything.
- **Garbage collection** is a compaction filter. A filter is created per compaction run and receives the run's
  items in key order (`lsm-tree`'s `CompactionFilter::filter_item(&mut self, ..)`), so it sees each key's versions
  newest first and drops those older than the newest discard-flagged or tombstone version at or below the discard
  timestamp, Badger's rule (`badger/levels.go:815-835`); the tombstone itself is dropped only when compacting into
  the last level, and no item above the last level is destroyed without a tombstone, so an older copy below cannot
  return. Versions outside a run are kept until a run covers them.
- **Prefix drops** write a point tombstone for every key under the prefix, in batches through the writer, then clear
  the cache; compaction reclaims the space, and lsm-tree's `drop_range` can drop tables wholly inside the prefix.
  fjall 3.1 offers no range deletion; dropping everything uses `Keyspace::clear`.
- **Banned namespaces** are a persisted set; a batch drops the writes addressed to them and writes the rest, and reads
  filter them. Badger refuses such writes with `ErrBannedKey`, which Dgraph ignores (`draft.go:984`).
- **Durability.** A fjall write batch flushes to its journal only as its persist mode says; every commit batch is
  written with the mode stage 4's durability setting chooses, and after a crash the store's applied timestamp names
  the last batch it holds.

Alternative: redb. Decided against on 2026-10-20 (`dgraph-port-overview.md`).

### The commit

- A request owns a scratch layer per touched list: Dgraph's `MutableLayer.currentEntries`, readable by the
  request at its own read timestamp. Index, reverse and count maintenance read and write through it.
- Commit marshals each touched list's scratch layer as a delta and writes all of them, the index, reverse and
  count deltas, any schema entries and the applied commit timestamp, as one fjall batch at the commit timestamp,
  then updates the cache under the same writer lock.
- Not ported: `pendingTxns`, the oracle's waiters, `ProcessDelta`, `shouldAbort`, `ResetTxns*`,
  `detectPendingTxns`, the `deltas` and `maxVersions` maps, `FillContext`. Conflict keys (`GetConflictKey`) are
  ported in stage 3 with the commit decision, where they belong.
- One writer per store applies commits in order. The applied timestamp advances after each batch and its cache
  update, and a read at `t` waits until every commit timestamp assigned at or below `t` is applied.

### The cache

moka keyed by the list's key, holding an `Arc` of the list and the commit timestamp it reflects. A commit replaces
or removes the entries of the lists it wrote before the writer lock is released; a reader at `t` uses an entry only
if it reflects every commit at or below `t`, which the applied timestamp tells it. A reader that missed inserts its
list only if no commit to that key was applied after its snapshot, checked under the writer lock; Dgraph inserts
unconditionally (`mvcc.go:883-891`), which lets a list read before a commit replace the commit's entry. Readers above
a list's newest version, the common case, therefore hit; in Dgraph they miss, because a hit requires
`readTs ≤ maxTs` of that list (`mvcc.go:776-781`, the fix for issue #9597).

### Rollups

A background thread rolls up lists in batches, as Dgraph's `incrRollupi` does (priority for lists with more than
500 deltas): it takes a fresh timestamp, waits until every commit below it is applied, rolls up there, and writes each
rollup as its own batch through the same writer (`mvcc.go:92-117`). A rollup leaves the versions below it readable,
so open reads bound only the discard timestamp. A rollup that removes split parts deletes their keys. Rollups pause
while an index rebuild runs.

### Rebuilds and drops

Ported as Dgraph has them: the `needs*Rebuild` decision tables, the dual schema (`mutSchema` for writes, the
interim query schema of `GetQuerySchema` for reads, `index.go:1132-1160`), the two-phase rebuild through a temporary
store (vector indexes rebuild in place, as `RunWithoutTemp` does), and type changes with their refusals. Two parts
differ:

- A failed rebuild keeps the previous schema's indexes complete. Dgraph drops indexes and rewrites list values before
  the build and restores only the schema record on failure (`worker/mutation.go:120-129`, `262-275`); the port builds
  into the temporary store first and drops the old entries only after the build succeeds.
- Drops remove split parts too; Dgraph's `DeletePredicate` and `DeleteData` drop only the default prefix and leave
  split parts behind (`index.go:1881`, `1894`). Dropping everything in a namespace also removes its schema and type
  keys, which Dgraph removes in memory only (`schema.go:150-168`).

## Risks / Trade-offs

- [fjall's compaction filter sees only one run's items] → Versions spread across levels are collected late, as
  in Badger. Measured on the workloads in the overview's "What to measure early".
- [One writer per store] → The commit rate is bounded by one thread's batch rate. Dgraph's apply path is serial
  per group already; the writer batches commits from one apply step.
- [Updating cached lists in place] → Dgraph mutates shared maps unless `refresh=true`. Lists in the cache are
  immutable `Arc`s, replaced on write.
- [HNSW's in-memory caches `nodeAllEdges` and `deadNodes`] → Dgraph loads `deadNodes` once and never refreshes it
  (`helper.go:748-768`). Both follow commits as the posting cache does, invalidated by the same writer. The dead list
  itself is written by index maintenance (`posting/index.go:113-152`), so it belongs to `store_apply`.

## Migration Plan

New crates; stage 1's `core_schema` gains a storage-backed loader. Go-written Badger directories are not read;
data moves through export and the loaders (stage 5).

## Open Questions

- Does the compaction filter alone keep version garbage bounded under Dgraph's rollup rate, or is a background
  sweep over hot keys needed as well?
- Which store holds a rebuild's temporary entries: memory with spill to disk, or a fjall keyspace from the start?
