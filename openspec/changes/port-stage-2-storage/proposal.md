## Why

Dgraph stores everything as posting lists in Badger, versioned by commit timestamp, and its stale reads come
from the layers it built above Badger: deltas held in memory until Zero decides, a cache updated after the write,
and a read gate beside the store's own versions. Stage 2 ports the posting lists onto fjall and replaces those
layers with one rule set, in one process and without a network, so the storage semantics are proven before a
query engine or a Raft log depends on them.

## What Changes

- A store over fjall that provides what Dgraph's managed-mode Badger provides: caller-supplied versions in the
  key, escaped so that fjall's byte order is Badger's order, a meta byte and discard flag per version, reads at a
  timestamp through one snapshot, per-key version iteration, prefix scans over a snapshot, atomic write batches whose
  entries each carry their version, physical prefix drops, banned namespaces, and garbage collection below a discard
  timestamp by a compaction filter.
- Posting lists ported from `posting`: the immutable part, deltas by commit timestamp, delete-all markers,
  rollups and split lists.
- The uncommitted delta layer is gone. A request's writes are
  built in a scratch layer visible to that request and to the requests applied after it in its batch, and written in
  one batch at its commit timestamp; the
  pending-transaction table, the oracle's waiters and `ProcessDelta` are not ported.
- The posting cache on moka, behind a trait, updated in the same critical section as the write it reflects.
- Index, reverse and count maintenance, schema changes with background index rebuilds, and drops.
- The HNSW vector index from `tok/hnsw`, stored in posting lists.
- **Behaviour changes (Dgraph bugs fixed):**
  - Dropping a predicate or a namespace's data removes its split-list parts, and a rollup deletes the parts it
    removes; Dgraph leaves both behind.
  - Dropping everything in a namespace removes its schema and type keys; Dgraph removes them in memory only, and
    they return at restart.
  - A read below the discard timestamp is refused with `Transaction is too old`; Dgraph's posting-list reads return
    what survived collection, silently (`mvcc.go:892-918`).
  - A failed rebuild leaves the previous schema's indexes complete; Dgraph restores the schema record over indexes
    already dropped.
  - A predicate created by a write is created in that write's commit batch; Dgraph creates it at the start timestamp
    and keeps it after an abort.
  - A read below an empty rollup reads the older versions; Dgraph serves the empty list to it (`mvcc.go:708-709`).
  - A star delete removes the tokens of language-tagged values; Dgraph tokenizes them without their language and
    leaves their tokens.
  - Deleting one value of a list predicate keeps the subject under the tokens its other values still produce; Dgraph
    removes it from every token of the deleted value.
  - A star delete on a scalar with language values removes the subject from count 1, where `@count` lists it; Dgraph
    removes it from the count of all its postings and leaves it under count 1.
  - A rollup and a commit are each one atomic batch; Dgraph writes a rollup key by key, so a crash can keep a main
    list without its parts, and splits a large commit across Badger transactions.
  - A rollup that read a list before a drop covering it writes nothing; Dgraph's can write the dropped data back.
  - Index maintenance reads every list it changes whole: a list first written without a store read is completed from
    the store before it is read, and a scalar list is read through the merging iterator; Dgraph's `GetFromDelta` and
    `GetScalarList` paths can miss the old value and keep its tokens (by reading).
- **Behaviour changes (the port's design):**
  - During a rebuild, writes maintain the previous schema's indexes as well as the new schema's, and one atomic batch
    swaps them when the build succeeds; Dgraph maintains only the new schema's.
  - A refused type change alters nothing, and a predicate holds data exactly when one of its data lists is non-empty;
    Dgraph refuses a list-to-scalar change on an empty predicate after dropping its indexes, and its `hasEdges`
    counts any key whose newest version is not empty.
  - A request whose read timestamp is below a drop's or schema change's timestamp, and that writes under it, fails at
    commit with `Transaction has been aborted. Please retry`; Dgraph aborts its pending transactions on namespace and
    full drops and refuses a predicate drop or schema update while pending transactions touch the predicate.
  - A batch writing to a banned namespace writes its other keys; Dgraph loses every write of that Badger transaction.
    Dropping everything clears the banned set, the discard timestamp and the applied timestamp; Dgraph keeps the
    banned set in memory until a restart.
  - The discard timestamp never exceeds the oldest open read; Dgraph bounds it by the oldest pending transaction and
    by change data capture.
  - A rollup writes its split parts at the main key's version; Dgraph writes them one version below.
  - A vector index rebuilt in place reads each key's merged list; Dgraph reads each key's newest version alone.
  - A write whose stored key, escaped and versioned, exceeds 65,535 bytes is refused, besides one whose user key
    exceeds Badger's 65,000 bytes; a key dense in zero bytes reaches the first limit below 65,000 bytes.

- A `Store` trait between the port and fjall, synchronous and shaped in batches, with a simulated implementation that
  injects storage faults (failed batches that poison the store, process crash, power loss, failed and corrupted reads,
  slow operations) under a seeded, deterministic scheduler and checks invariants against a shadow model after every
  step.

## Capabilities

### New Capabilities

- `kv-store`: the versioned store over fjall and its garbage collection.
- `storage-simulation`: the `Store` trait, the simulated store and its faults, seeded deterministic runs, invariants.
- `posting-lists`: reading, merging, rolling up and splitting posting lists.
- `local-commit`: a request's edges applied over its earlier edges, committed in one batch at its commit timestamp.
- `posting-cache`: the moka cache and when it is updated or cleared.
- `index-maintenance`: index tokens, reverse edges, counts and delete-all, kept with each write.
- `schema-changes`: schema updates, index rebuilds, list-type changes and drops.
- `vector-index`: the HNSW index and its search.
- `schema-storage`: the schema state saved to and loaded from the store.

### Modified Capabilities

None. `schema-storage` extends stage 1's `schema-parsing` without changing its requirements.

## Impact

- New crates `ultragraphdb_store_*` in the `ultragraphdb_store/` group, with `ultragraphdb_traits_store` and
  `ultragraphdb_error_store`; `fjall` (pinned to 3.1.12) and `moka` join `[workspace.dependencies]`;
  `ultragraphdb_common_utils` gains `go_farm`, an in-house port of go-farm's `Fingerprint64`.
- `ultragraphdb_core_schema` gains loading and saving through the `Store` trait.
- The workspace `clippy.toml` bans the wall clock, thread spawning and `RandomState::new`, and `make check` fails on a
  disallowed method.
- The commit path differs from Dgraph's by design; stages 3 to 5 build on the single-batch commit.
