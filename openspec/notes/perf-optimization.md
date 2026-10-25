# Performance of the apply path

The apply thread is the port's write ceiling: in OrbStack's Linux VM it saturates near 22–23k relations/s, at 40–45 µs
of CPU per relation, whatever the relations per commit (`dgraph-port-overview.md`, "Measured, stage 2"). Dgraph applies
a group's proposals one at a time on one goroutine too (`worker/draft.go:863`, `processApplyCh`), splitting a
mutation's edges across goroutines only from 512 edges (`draft.go:561`, `x.DivideAndRule`), and one group reached about
79k relations/s at 50 per commit (`dgraph-bench.md` in scrith): at most about 12.7 µs per relation. The port spends
about three times as much per relation. This note sets out where that time goes and the options for each part; the
work is its own change set.

## Where the time goes

A stack sample of the apply thread at saturation (macOS, 50 relations per commit, `sample` over 12 s), self time:

| Share | Where |
|---|---|
| 28 % | allocation and freeing |
| 19 % | `memcmp`, `memmove` |
| 18.5 % | lsm-tree: decoding data and index blocks, seeks |
| 3 % | fjall's memtable skiplist |
| about 8 % | the port's posting, apply and kv code |

Inclusive, 49 % of the apply thread is inside fjall and lsm-tree, allocation and comparisons included: 34 % in seeking
a key's versions on a list read, 7.7 % in the commit's write.

Counters on the read path of the same workload settle what those reads are. At 50 relations per commit: 1,300,000
list reads, every one a cache miss, 1,299,999 of them of a key with no version at all; at one relation per commit:
1,000,000 reads, all misses, 999,999 of keys never written. About 1.9 list reads per relation, each a new subject's
key: the cache never sees these keys before they are written, so no cache serves them. The cost is proving a key absent.

The allocation trims on the commit and cache-miss paths (no delta copied to collect written keys, the commit borrowing
each request's deltas, a miss keeping one copy of its key and moving its lists) gained about 1 %: the port's own
allocation sites are each at most about 1 % of the thread. The rest of the allocation is lsm-tree's merging iterator
and block decoding, built afresh for every list read.

## Why the version is part of the key

Dgraph runs Badger in managed mode (`worker/server_state.go:80-126`): every write carries a timestamp the caller
supplies, Zero's commit timestamp, and one key holds many versions at once:

1. **Snapshot reads.** A query or a request reads at its read timestamp and sees, for each key, the newest version at or
   below it, while newer commits land.
2. **Deltas.** A commit writes its change to a list as a delta version at its commit timestamp; a read merges the
   complete version with the deltas above it (`posting/mvcc.go`, `ReadPostingList`).
3. **Rollups.** A rollup writes a complete version at a timestamp below deltas already stored, so a read finds it under
   them.
4. **Collection and deletion.** Versions below the discard timestamp are collected; a deletion is itself a version, a
   tombstone, that hides the versions below it.

fjall versions its writes with sequence numbers it assigns itself, increasing and local to one process. Dgraph's
versions are chosen outside the store and must agree across replicas from stage 6, and a rollup writes a version older
than versions already stored. So the version is part of the stored key: the user key escaped (`0x00` as `0x00 0xFF`),
the terminator `0x00 0x01`, then the complement of the version in big-endian order. A key's versions are then
contiguous, newest first, and "the newest version at or below `t`" is one seek, through a merging iterator, because a
rollup can write a version below versions in higher levels (stage-2 design, "The store").

Badger keeps the same layout and still answers most reads of absent keys from its bloom filters, because its table
builder hashes the key without its timestamp (`badger/v4@v4.9.6/table/builder.go:210`, `y.Hash(y.ParseKey(key))`) and a
read consults the filter before it touches the table (`table/table.go:674`, `DoesNotHave`). lsm-tree's filters hash the
whole stored key and serve point gets alone (`lsm-tree-3.1.10/src/table/mod.rs:281`); a range read consults none, and
3.1.10 has no prefix extractor. Every list read of an absent key therefore seeks the index and data blocks of each table
whose key range covers it.

## Options for proving a key absent

| Option | What it is | Gain | Cost |
|---|---|---|---|
| A. Existence marker | A key without a version, written in the same batch as a key's first version and removed only with its last | A point get the bloom filter answers proves absence; up to the 34 % spent seeking absent keys | One more key per list; drops, collection and the rebuild swap must remove markers |
| B. Prefix filters in lsm-tree | Bloom filters over the escaped user key and terminator, as Badger's over the key without its timestamp | The same as A, with no extra keys | A release of lsm-tree that offers prefix filters, or a fork |
| C. In-memory key filter | A filter over every key stored, rebuilt at open | The same as A, with no store change | Memory per key, a full scan at every open, and a second structure that must agree with the store |
| D. fjall's own MVCC | Versions as fjall sequence numbers | Point gets with filters | Not possible: fjall assigns sequence numbers itself, and a rollup writes below stored versions |

**A, the existence marker,** holds one invariant: a key without a marker has no versions. The marker is written in the
write batch that writes a key's first version, so a crash or a power loss leaves both or neither; a read whose point get
finds no marker returns absent, and a read that finds one seeks the versions as today, so a marker left after its key's
versions were removed costs a seek and never a wrong answer. A drop removes the markers under its prefixes with the
versions; collection and the rebuild's swap remove a key's marker in the batch that removes its last version. The
simulator's shadow model takes the invariant as a rule after every step and every recovery. Markers would live in a
keyspace of their own, so that their bloom filters hold no versioned keys, and a drop removes the markers under its
prefixes there as it removes the versions in the data keyspace.

**B** removes the extra keys but depends on lsm-tree, which is pinned at 3.1.10 with fjall 3.1.12 because
`Store::compact` calls fjall's `#[doc(hidden)]` functions.

## Parallel options

The port applies as Dgraph does: the requests of a batch in order, each reading what earlier ones wrote through the
stacked layers, and a request's edges in forks per predicate only when Dgraph would run more than one goroutine. The
store write takes 7–13 % of a batch; the rest is the apply thread's.

| Option | What it is | Gain | What it must keep |
|---|---|---|---|
| P1. Read-ahead | The lists a batch's requests will read, known from their edges (data, reverse, index and count keys), read on executor threads at each request's read timestamp before the request runs | Overlaps the absence proofs and list decoding, about half the thread, with apply | The reads stay snapshot reads; the apply order is unchanged; a key read ahead and then written by an earlier request in the batch is read again through the stacked layers |
| P2. Independent requests in parallel | Requests of a batch whose key sets are disjoint applied concurrently, then committed in order | Scales with cores while requests are independent | Key sets known before the request runs: upsert blocks and conditional requests read keys their query finds, so they run in order; a refused request still refuses its dependents |
| P3. Smaller splits within a request | Forks per predicate below Dgraph's 512 edges | Uses cores for a mutation across several predicates | A fork is a request of its own, absorbed back after it runs, so a small mutation may lose; measured before any threshold is chosen |
| P4. Pipelined batches | Batch n+1 applied while batch n is written to fjall | Up to the 7–13 % write share | The cache and the applied timestamp update after the write, as now; batch n+1 reads batch n's lists through the stacked layers, not the store |

All four keep one writer: the store batch is still one atomic batch per apply step, in commit order, which is where the
roadmap's single-writer idea (`ROADMAP.md`, "Improvements, single node") puts the writer. The simulator runs every option
under `SteppedExecutor`, so a seed still replays a run; P1 and P2 move work onto the executor, which the simulator
already schedules.

## Allocation still on the path

- The commit hook copies each request's written keys for its fence check and its refused set
  (`ultragraphdb_store_apply/src/types/applier/applier_commit.rs`); the copy is needed only when fences exist or the
  request is refused.
- Every list read builds its stored key and its end key (`ultragraphdb_store_kv/src/utils/stored_key.rs`), a snapshot,
  and lsm-tree's merging iterator; A or B removes most of these reads.
- A cache hit clones its list whole (`lists_read.rs`, `cached`); the measured workload has no hits, a read-heavy one
  does.

The sample was taken on macOS, whose allocator differs from glibc's; the store ships against glibc, so allocation is
measured again in OrbStack before any of it is tuned.

## Measuring

`store_bench apply --dir <dir> --rate 50000 --relations 50 --seconds 30` in OrbStack, as in the overview, with the
same store, cache and batch settings; three runs per row. A change to the read path reports the list reads per
relation, the cache hits and the reads that found nothing, beside the commit rate.

## Open questions

- Does a release of lsm-tree after 3.1.10 offer prefix filters, and at what cost to fjall's pin?
- Does the existence marker pay for itself on a workload that mostly reads existing keys, where it adds a point get to
  every miss?
- Which of P1 and P2 belongs in the single-node change's apply thread (task 7.2 of `port-stage-5-single-node`), and
  which in this change set?
