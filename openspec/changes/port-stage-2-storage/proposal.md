## Why

Dgraph stores everything as posting lists in Badger, versioned by commit timestamp, and its stale reads come
from the layers it built above Badger: deltas held in memory until Zero decides, a cache updated after the write,
and a read gate beside the store's own versions. Stage 2 ports the posting lists onto fjall and replaces those
layers with one rule set, in one process and without a network, so the storage semantics are proven before a
query engine or a Raft log depends on them.

## What Changes

- A store over fjall that provides what Dgraph's managed-mode Badger provides: caller-supplied versions in the
  key, a meta byte and discard flag per version, reads at a timestamp, per-key version iteration, prefix scans
  over a snapshot, atomic write batches at one version, prefix drops, banned namespaces, and garbage collection
  below a discard timestamp by a compaction filter.
- Posting lists ported from `posting`: the immutable part, deltas by commit timestamp, delete-all markers,
  rollups and split lists.
- **BREAKING (relative to Dgraph's internals):** the uncommitted delta layer is gone. A request's writes are
  built in a scratch layer visible to that request alone and written as one batch at the commit timestamp; the
  pending-transaction table, the oracle's waiters and `ProcessDelta` are not ported.
- The posting cache on moka, updated in the same critical section as the write it reflects.
- Index, reverse and count maintenance, schema changes with background index rebuilds, and drops.
- The HNSW vector index from `tok/hnsw`, stored in posting lists.
- **Behaviour change (Dgraph bug fixed):** dropping a predicate or a namespace's data removes its split-list parts,
  which Dgraph leaves behind.

## Capabilities

### New Capabilities

- `kv-store`: the versioned store over fjall and its garbage collection.
- `posting-lists`: reading, merging, rolling up and splitting posting lists.
- `local-commit`: a request's writes, visible to itself, committed as one batch at one timestamp.
- `posting-cache`: the moka cache and when it is updated or cleared.
- `index-maintenance`: index tokens, reverse edges, counts and delete-all, kept with each write.
- `schema-changes`: schema updates, index rebuilds, list-type changes and drops.
- `vector-index`: the HNSW index and its search.
- `schema-storage`: the schema state saved to and loaded from the store.

### Modified Capabilities

None. `schema-storage` extends stage 1's `schema-parsing` without changing its requirements.

## Impact

- New crates under `ugdb_store/`; `fjall` and `moka` join `[workspace.dependencies]`.
- `core_schema` gains loading and saving through the store.
- The commit path differs from Dgraph's by design; stages 3 to 5 build on the single-batch commit.
