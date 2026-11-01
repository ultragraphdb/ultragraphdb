# UltraGraphDB Shard-per-Core Write Path

Preparation for stage 5.5's specifications: what bounds a replica group's write throughput, measured on 2026-11-01,
and the runtime model the measurements point to. A storage node runs one partition per core, each a shared-nothing
replica group with its own log, commit step and store, after ScyllaDB's shard-per-core model and TigerBeetle's
one core per replica. Backlog item 11 tracks what remains to specify; the distribution architecture
(`UltraGraphDB Distribution Architecture.md`) is the frame it refines.

## Measurements

All figures come from `wal_bench` (the stage-5 pipeline: execute, validate, log, apply) on the OrbStack VM
`ugdb-linux`: aarch64, 12 vCPUs on a host with 12 performance and 4 efficiency cores, glibc with mimalloc, data on the
VM disk (`/var/tmp`). The VM's `/tmp` is a RAM-backed tmpfs where `fdatasync` costs about 1 µs, so runs there
overstate throughput. Each commit of `r` relations creates `r` new nodes, each with an edge to one of the hubs and a
string property.

### One commit thread

| What | Figure |
| --- | --- |
| Relations per second, one pipeline | about 38,000 at 10 and 50 relations per commit; 16,700 commits/s at 1 |
| CPU on the commit thread | about 35–60 µs per commit plus 20–25 µs per relation |
| Store operations per relation (50 per commit) | 2.0 dictionary point reads, 1.0 of them a miss (the new node's IRI); 5.05 entries written; no snapshot reads |
| Cost per operation | about 3 µs per point read, about 1 µs per memtable insert |

The profile (`wal_bench_profile`, `perf record -g`, 50 relations per commit) splits the commit thread's time:

| Step | Share | Runs in parallel? |
| --- | --- | --- |
| Executing transactions (`Transaction::write`) | 39%, the dictionary's IRI lookups 24% | Yes: each transaction reads its own snapshot |
| Preparing commits (`Transaction::commit`) | 8% | Yes |
| Applying rounds (posting writes, fjall's batch commit) | 37% | Yes, by key range or keyspace: the memtable is a concurrent skiplist |
| Validation and `Execution::finish` | 11% | No: the commit step's serial core |

An operation costs microseconds because each is a walk down a large crossbeam skiplist: every level is a cache miss,
and a key longer than 20 bytes adds a second miss to reach its bytes. Dictionary keys are hashes, so successive lookups
share no cache lines. The cost is the number of operations per relation, each paying that walk; the bytes of the keys
add little.

### The stored key and the dictionary cache

Two changes landed in `ultragraphdb_store_kv` on 2026-11-01:

- **The stored key encodes zero runs.** A run of zero bytes takes two bytes however long, in place of two bytes per
  zero byte (`kv-store` spec, *Versions are part of the key*). Dgraph's keys are mostly zero bytes, so the change
  shortens them and cuts the fjall journal by 23% per relation.
- **The keyspace without versions caches what its point reads find**, bounded at 64 MiB, invalidated by every commit
  that writes it and by a drop of everything (`kv-store` spec, *A keyspace without versions*). Lookups of known IRIs,
  hubs and predicates among them, become hash lookups; a new IRI still reads the store.

Measured against the preceding build, three runs each:

| Relations per commit | Before | After | Change |
| --- | --- | --- | --- |
| 1 | 13,357 | 15,696 | +17.5% |
| 10 | 34,161 | 41,217 | +20.7% |
| 50 | 36,397 | 40,491 | +11.2% |

Smaller memtables were tried and rejected: 16 MiB gained 4% at 50 relations and lost at 10, 8 MiB lost 31% to flush
pressure.

### Partitions side by side

N independent pipelines in one VM, each with its own log, fjall instance and commit thread, measure what one
partition per core yields. Relations per second:

| Pipelines | 1 relation per commit | 10 relations | 50 relations |
| --- | --- | --- | --- |
| 1 | 16,706 | 39,043 | 38,501 |
| 2 | 26,400 | 76,677 | 77,047 |
| 4 | 55,755 | 139,509 | 141,021 |
| 6 | 62,806 | 192,503 | 201,654 |
| 8 | 88,189 | 241,292 | 199,514 |
| 10 | 99,412 | 271,578 | 252,924 |
| 12 | 107,130 (6.4×) | 276,836 (7.1×) | 263,196 (6.8×) |

Each pipeline loses 41–47% of its own throughput at 12, at every commit size. The pipelines share no lock, so the
loss is the machine's: 12 commit threads, fjall's flush and compaction workers and the load generators on 12 vCPUs,
and every fsync on one virtual disk. At one relation per commit the fsync per commit dominates and scaling is weaker
from the start. The relations set one core's ceiling; partitioning sets how many cores it multiplies by.

A single commit step fed by execution on every core is bounded by what stays serial. By Amdahl's law from the profile:
about 1.9× with execution off the commit thread, about 2.7× with the apply on a thread of its own, and above that only
with the apply itself split, until validation's 11% bounds it.

## The model: one partition per core

**References.** ScyllaDB runs one shard per core on Seastar: each shard owns a share of the data and its own memory,
and shards exchange messages over per-pair queues, never shared structures. TigerBeetle runs each replica
single-threaded on one core, batching thousands of operations per request through io_uring. Both put the serial
section of a write on one core that owns its data, and scale by adding owners.

**Shape.** A storage node runs one partition per core. A partition is a replica group: its own VSR log, commit step
and LSM instance, its members one core on each of three nodes. Each core's compio runtime owns its I/O queues, as the
distribution note's runtime model states; with partitions, the commit step is per core too.

**What the model changes in the clustering architecture.**

1. **Placement follows the bucket.** Predicate sharding puts a hot predicate on one partition and so on one core:
   about 40,000 relations per second for that predicate, whatever the cluster's size. Placement by the D1 bucket of
   the subject's ID spreads a predicate across partitions. An edge's forward entry then lives with its subject and its
   reverse entry with its object, usually elsewhere; the reverse follows by change stream, the mechanism the
   distribution note plans for tiles, needed from the first partitioned stage.
2. **The term dictionary is a partitioned unique index.** An IRI's uniqueness is checked by the partition owning its
   hash. Deriving an entity's bucket from the same hash, of its IRI or of its UGS `@key`, keeps creation and the check
   in one partition.
3. **Replica groups multiply**, nodes × cores ÷ replication. VSR traffic between two nodes shares one connection,
   heartbeats coalesce and idle groups quiesce, as multi-Raft systems found necessary. The placement map grows, and
   closed-epoch markers reach the sequencer batched per node.
4. **A transaction spanning partitions on one node takes the cross-group path**, Calvin sequencing and epochs over
   shared-memory queues. The parity rule holds: stage 8's single container runs the paths a cluster runs. D3's
   guarantees apply per partition: strict serializability within one, serializability at a closed epoch across them.
5. **Each partition has its own LSM instance.** A shared fjall journal would serialize the partitions again. fjall's
   flush and compaction workers and the memory of memtables and caches take a budget per node, divided among the
   partitions.
6. **Rebalancing is one mechanism**: a bucket moving between cores is the operation that moves it between nodes.

The three roles stay as they are. Stage 5.5 settles the identity (D1, the bucket's meaning and derivation) and the
metadata group (D4, more groups to place); stage 8 builds the partitions, the reverse entries by change stream and the
VSR multiplexing.

## Open questions for stage 5.5

- Is the bucket derived from a hash of the IRI or `@key` for every entity, and what does a keyless entity hash?
- How many buckets per partition does the placement map start with, so that a node's partitions can be rebalanced
  across cores without splitting a bucket?
- Which writes stay single-partition under bucket placement: an edge insert with its reverse entry by change stream,
  a property write, an entity creation; and which become cross-partition: unique keys of another entity,
  cardinality on the object's side?
- Does a node run as many partitions as cores, or reserve cores for fjall's workers, the frontend role and the network,
  given that the side-by-side runs lost 41–47% per pipeline at 12 on 12 vCPUs?
- How does `wal_bench` grow to measure the model in one process: N partitions on N pinned compio runtimes over one
  shared generator, with cross-partition commits at a chosen share?
- Does each partition's commit step keep the apply on its own core, or apply on a second core pipelined with the next
  round, given the apply's 37%?
