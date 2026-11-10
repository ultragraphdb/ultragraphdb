# UltraGraphDB Scalable Graph Storage Engine

Nov 10, 2026 · first design draft; decisions T1–T29 adopted 2026-11-10

The storage engine of roadmap stage 6.5: a graph-native tile store that replaces the fjall KV store whole, writes
included. A partition keeps its graph as tiles, bounded and checksummed blocks of three shapes: adjacency blocks of a
relation's sparse matrix, column chunks of a type's attributes, and sorted record runs for dictionaries and small
indexes. Each tile is an immutable base on disk with a delta in memory. The write-ahead log stays the only stream synced
per commit; deltas fold into new tiles written out of place, and a checkpoint publishes them under a superblock written
last. The schema compiles the physical layout and gives every relation and attribute a storage class, so the engine
spends serialization only where UGS declares it is needed: append-only and immutable data take no MVCC copies, no pins
and no read-set entries, and only serializable read-write work pays for validation.

The design combines what the sources below measured or built: GraphScope Flex's typed, dense, lock-free adjacency, its
date-sorted lists and functional edges; Vineyard's sealed blobs and in-tile CSR geometry; Galaxybase's sorted per-vertex
edge pages and atomic copy-on-write activation; TigerBeetle's grid, deterministic compaction, superblock and
checksummed references; the TU Munich and TigerBeetle results on io_uring and on writing to SSDs; and TigerStyle's
limits, static allocation and assertions. It keeps P24's placement (one owner per fact, referrer sets) and the commit
step, and it changes whatever beneath them the engine needs: the user stated on 2026-11-10 that the threading model and
the KV-era crates may be redesigned for it.

Nothing here is built or measured. Every figure for the new engine is an estimate and says so; every figure for another
system names its source.

## Summary

| Area | KV store today (measured) | Tile store (design) |
|---|---|---|
| Records written per relation | 9.1 store versions | 2 delta entries in memory, statistics per round |
| Storage CPU per relation | 42–44 µs (16.5–20 on the commit thread, 25–27 on fjall's workers) | about 1–2 µs estimated: apply 0.3–1, compaction 0.3–1 amortised |
| Bytes to disk per relation | about 1.9 KB before LSM compaction, which multiplies the store's share by an assumed 5–10× | about 200 B estimated: the logical WAL record and the encoded share of rewritten tiles |
| A cached one-hop read | a posting-list fetch, about 3 µs, 25–90 µs when fjall loads a block | a directory search and a row decode, about 0.2–0.5 µs plus 1–2 ns per neighbour, estimated |
| Snapshot pin | 5–20 µs of storage CPU per partition per read | a counter per epoch, nanoseconds, estimated |
| Recovery | fjall's journal and the WAL above the checkpoint | the superblock, the manifest and the WAL above the last checkpoint, bounded in bytes |
| Corruption | block checksums inside fjall | every tile checksummed in its header and in every reference to it; repair from a replica's identical tile |

The engine's eleven ideas, each a decision in *Decisions*:

1. **Graph-native, whole.** Adjacency, attributes and indexes live in tiles; no KV layer remains beneath or beside them (T1).
2. **Three tile shapes on one versioning scheme** (T2): adjacency blocks, column chunks, record runs, each a base plus a
   delta.
3. **Dense local slots** (T3): an entity's 128-bit ID maps once to a 32-bit slot of its partition, which indexes its
   columns and names it in local reverse lists. Slots never leave the partition.
4. **The schema compiles the layout and the storage class** (T7): typed adjacency per (relation, direction), single-valued
   roles as flat columns, small edge attributes inline, immutable attributes denormalised into adjacency where they sort it.
5. **Version-complete tiles** (T8): entries carry versions only while a reader can need them; append-only tiles answer
   every epoch from one base.
6. **Storage lanes** (T9): the query and write classification picks a lane, and the lane decides what the engine does:
   pin or not, filter versions or not, record a read set or not, validate or not.
7. **Memory-only apply, out-of-place tiles, superblock last** (T5): the WAL is synced per round; tiles reach disk through
   paced compaction and checkpoints.
8. **Deterministic compaction and placement** (T6): replicas' grids are byte-identical at every checkpoint, so a corrupt
   tile is repaired from a peer by address and checksum, and a replica catches up by copying tiles.
9. **Running conservation invariants** (T11): every tile carries an order-independent digest of its entries; compaction
   must conserve it, and each relation's forward and reverse digests must stay equal.
10. **Shared-nothing per core** (T10): the engine of every replica runs on its core's thread, with no lock, no atomic on
    the data plane and all I/O through that core's io_uring ring.
11. **SSD-aware writing** (T13): tiles compressed and packed into 4 KiB slots, grouped by expected death time, space
    reclaimed by extent apart from logical compaction, the WAL in its own files, the drive never filled.

## 1. The baseline

`openspec/benches/benches-2026-11-09.md` is the KV baseline this engine is measured against, and backlog item 19 states
its floor. Through the three binaries at one partition the commit thread spends 30.4 µs a relation and 16.0 µs a commit;
the store holds 16.5–20 µs of each relation on that thread and fjall's workers spend 25–27 µs more flushing and
compacting, 42–44 µs of CPU a relation in all. A relation writes 9.1 store versions: 2.0 forward, 2.0 reverse, 1.0
subject-predicates, 1.1 object-predicates, 2.0 statistics and 1.0 referrers. Validation, what strict serializability
costs, is 0.8% or less. A cold Scan spends 89% of its time loading and checksumming a fjall block per list.

The cost is structural: every derived entry of a graph write is a key in a sorted map, each insert a walk down a
skiplist whose every level is a cache miss, each later rewritten by flushes and compactions that know nothing of the
graph. Backlog item 19 asks three questions, which this design answers: which lists a tile holds together so a relation
writes fewer than 9.1 records (*Adjacency tiles*, *Record runs*); whether statistics can be kept per round and still be
exact at every version (*The write path*); whether a tile is compressed on the commit thread (never: *Compaction*).

## 2. What the sources teach

### The state of the art

LDBC SNB Interactive v1, audited results read from ldbcouncil.org on 2026-11-10 (the GraphScope research report):

| Date | System | Hardware | SF100 | SF300 | SF1000 |
|---|---|---|---|---|---|
| 2024-05-14 | GraphScope Flex 0.26.1, C++ stored procedures | 32 cores / 64 threads EPYC 9T24, 512 GiB, 4 TB ESSD | 130,098 ops/s | 131,264 | 127,785 |
| 2025-04-21 | GraphScope Flex 0.31.3, Cypher | 96 cores / 192 threads, 768 GiB | 79,244 | 80,511 | — |
| 2025-12-01 | Huawei Cloud | — | 129,090 | 139,411 | 125,873 |
| 2026-09-30 | Galaxybase (CreateLink) | — | 138,507 | 147,382 | 135,335 |

In the GraphScope mix about 28% of operations are inserts and 65% short reads (its `Performance.md`, SF30). The 2024
run reaches about 4,070 operations a second per physical core; the same engine through its Cypher runtime about 825.
The two runs share a CPU family but not a machine size, a time-compression ratio or an on-time share, so the fivefold
gap indicates, and does not measure, what query execution costs beside storage. Both GraphScope runs are single
machines; the table states no hardware for the other two. Interactive v1 has no deletes; v2 adds cascading deletes,
which this engine must serve and the GraphScope store read here cannot.

### GraphScope Flex (`flex/storages/rt_mutable_graph`, read at `d8581c18`)

What produces the throughput, ranked by the research report: the whole graph in RAM with dense `u32` vertex IDs and one
dual CSR per (source label, destination label, edge label), so every hop is an array index; compiled C++ procedures over
typed views; readers taking no lock, an edge's visibility one inline `u32` timestamp compare; flat arrays for
many-to-one edges (`SingleMutableCsr`: a message's creator, `replyOf`, `isLocatedIn`), making short reads O(1);
adjacency sorted by `creationDate` at load with an unsorted tail scanned backwards (`foreach_edges_gt`), which answers
"latest N" queries without a full scan; small edge properties inline in the neighbour entry (`MutableNbr<Date>`, 16 B);
`ONLY_IN` and `ONLY_OUT` relations keeping one direction; a per-core arena and WAL.

What it trades away, every item a reason this design differs. The items were read in the code and not reproduced; the
races among them are inferred from the order of writes, and the audited runs passed LDBC's own ACID tests, which a
benchmark mix without conflicting updates does not reach. Vertices and their properties are unversioned, so a reader
sees an insert before its timestamp is released; an adjacency list publishes its size before the entry is written; the
read timestamp advances only over a contiguous prefix of released inserts, so a client can miss its own acknowledged
write; updates and compactions stop the world; `batch_commit` applies before it logs; no uniqueness, no deletes; fixed
capacity sized at open, its overflow raised after the WAL record is durable; strings silently cut at 256 bytes; a WAL
with no checksums, sparse files and no directory sync; no online checkpoint, so recovery replays all history (552 s at
SF300); the audit's durability test a process kill, which never reaches the drive. It does sync every WAL append with
`fdatasync`, without group commit, one WAL per core.

### Vineyard (v6d)

A read-optimised analytical snapshot format with no write path to learn from. Taken from it: a sealed, immutable blob
as the unit of sharing; dense local offsets with inner vertices first and outer (foreign) vertices after, so adjacency
stores 4-byte references; one CSR per (vertex label, edge label) sorted by neighbour; delta and varint encoding of
neighbour runs in batches of 16; columns per label indexed by offset. Left behind: its dense `tvnum + 1` offset arrays
for every label pair (16 B per vertex per edge label even where no edge exists), a partition ID inside every vertex ID,
which blocks rebalancing, rebuilding a whole label's CSR to add one edge, JSON metadata behind an etcd lock committed in
non-atomic chunks of 127 operations, spill files without checksums or sync, and shared-memory mappings its own Rust
client reaches only through `unsafe`.

### Galaxybase (PVLDB 17(12), 2024)

The paper behind the "GalaxyGraph" file. Taken: one edge page per vertex sorted by (direction, edge type, neighbour,
multi-edge index), which turns direction and type filters into range reads and a single-edge check into a binary search
(its Table 2: 10,270 against Neo4j's 3,183 type-filtered traversals, 49,017 against 19,592 single-edge queries); dense
sequential local IDs with relative 4-byte offsets; copy-on-write partition rewrites activated atomically once complete;
splitting a partition when its write I/O outgrows its read I/O and merging neighbours in the other case; per-query
memory and thread budgets. Left behind: a provisional transaction ID rewritten to a wall-clock commit timestamp, 2PL
with selectable isolation, asynchronous WAL shipping it calls eventual consistency, and whole-partition rewrites. Its
benchmarks are vendor micro-benchmarks on LDBC datasets, not the audited query mix; its audited Interactive result is
the table above.

### High-Performance DBMSs with io_uring (Jasny, El-Hindi, Ziegler, Leis, Binnig; PVLDB 19(9), 2026)

On one core, a buffer manager went from 16.5k to 183.5k transactions a second by keeping up to 128 I/Os in flight
through cooperative tasks, +18% with adaptive submission batching, +11% with registered buffers, +20% with NVMe
passthrough, +21% with polled completion (Fig. 5). Batches of about 16 cut cycles per operation 5–6× (§2.1), but a batch
of 128 raises a write's mean latency to 201 µs against 11.5 µs alone (Table 2). Single-issuer with deferred task running
is the recommended mode (§2.2). Writes above `max_hw_sectors_kb` (128 KiB with an IOMMU), above 512 KiB or past
`nr_requests` in flight fall back to kernel worker threads; 128 KiB writes and 256 KiB reads saturate PCIe 5 from one
core (§3.6). `fsync` always runs on a worker; linking it to the write gains nothing; `O_SYNC` is twice as slow as a write
and an `fsync` (Fig. 10). SQPOLL costs a second core and about 30 µs to wake. One ring for storage and network beats
libaio and epoll by 1.25–1.67× (Fig. 15).

For this engine: compio's runtime is built today with neither `single_issuer` nor `defer_taskrun`
(`ultragraphdb_common_service/src/utils/run_service.rs:315`), and compio-driver 0.12.5 has no `register_buffers` and no
fixed-buffer read or write.

### How to Write to SSDs (Lee, Ziegler, Leis; PVLDB 2026)

Total write amplification is the database's times the drive's, and the drive's comes from mixing data that dies at
different times in one erase unit. On a PM9A3 at 800 GB, writing out of place, compressing each page with LZ4 and
packing pages into 4 KiB slots, grouping pages by expected death time (from each page's last four write positions),
aligning the database's reclamation unit with the drive's, and filling open zones before opening new ones took the
flash bytes per page write from 4,378 to 567 and throughput from 229k to 535k operations a second (Table 1). Compression
did most of it (566 against 3,364 logical bytes an operation); writing out of place without the rest raised
amplification 1.66×. A compressed page crossing a 4 KiB boundary costs two reads (§3.2). Reclaiming space at 85% fill
gave a total amplification of 3.3 where using all of it gave 5.46 (§10.1). Out-of-place writing removes doublewrite
buffering, since the old version stays valid until the new one is durable (§10.6). Every drive measured has power-loss
protection, and the WAL is left out of every measurement.

### TigerStyle (tigerstyle.dev) and TigerBeetle

"Put a limit on everything because everything has a limit." Allocate all memory at startup; assert arguments, returns
and invariants, positive and negative space; abstract non-deterministic physical interfaces behind deterministic logical
ones; separate the control plane from the data plane by batching; sketch network, storage, memory and compute in
bandwidth and latency before building, since "the time to solve performance, and get the 1000x wins, is in the design
phase". `openspec/protocols/ULTRA_STYLE.md` already carries these rules for Rust.

TigerBeetle's storage, from its design as this repository has followed it (P10, the distribution protocol), not read
for this draft: a grid of blocks addressed by index and checksum, so a reference detects a misdirected or stale block;
blocks released at a checkpoint reusable only once the next checkpoint is durable; compaction paced in beats per batch
and deterministic, so replicas' grids are identical and a block is repaired from any peer; a superblock in several
copies, written last.

## 3. Targets and the envelope

**Targets.** Storage CPU at or below 2 µs a relation on the commit path and in compaction together, a twentieth of
today's; bytes to disk at or below 300 B a relation, WAL included; a cached one-hop expansion within twice the cost of
GraphScope's array walk; a cold tile read one I/O. Measured end to end, the stage's exit gate is the LDBC SNB
Interactive workload run with the official driver on the three binaries with every commit synced: v1, compared per
physical core against the table above, and v2, whose deletes the table's systems were not audited on. Storage cannot carry that number alone: the fivefold gap
GraphScope measured between compiled procedures and Cypher belongs to stage 7, and placement decides how many partitions
a short read touches (*Distribution*).

**Memory.** A delta entry is 32 B forward (neighbour 16, version 8, flags and an inline payload slot 8) and 16 B reverse
(subject slot 4, version 8, flags 4): about 48 B a relation. A delta pool of 256 MiB a replica, 768 MiB for a core's
three, holds about 5.5 million relations, 27 s of ingest at 200,000 relations a second, so compaction has slack without
a cliff. Followers hold the same deltas as their leader: a partition's memory is paid three times in the cluster. A tile
cache takes what remains of a core's budget; at SF300, about 850 million vertices and 5.3 billion edges (scaled from Galaxybase's
SF100 table), adjacency at about 8 B an entry in both directions is about 85 GB, so a 512 GiB node holds the topology
and the hot attributes.

**CPU.** Apply is one hash probe and one append per entry, about 50–150 ns each, against 1–3 µs a skiplist operation
today. An insert that must prove an absence first, a `@set` relation's combination or a key's claim, reads its row or
run: a cached tile costs a decode of the row, an uncached one an I/O on the commit's path, which the tile filters of
*Adjacency tiles* avoid for most absent keys. Compaction decodes, merges, encodes and checksums at an estimated 10–20 ns per entry, multiplied by the rewrite
factor `(1 + θ) / θ`, 3 at θ = 0.5: about 0.1–0.3 µs per entry, two entries a relation.

**Disk.** The WAL record is about 150 B a relation (P9's logical estimate); tiles add about two entries of 8 B, rewritten
three times, plus the inline and column bytes: about 200 B in all. At 200,000 relations a second a core writes about
40 MB/s; a 48-core node about 2 GB/s, sequential, in 128 KiB to 512 KiB writes.

**Network.** Unchanged by the engine: replication sends the WAL's logical rounds (about 150 B a relation to each of two
followers), and catch-up and repair send tiles, which compress.

## 4. The design

### 4.1 Shape

A storage node gives each partition core the replicas P8 places on it, leading one partition and following two. Each
hosted replica has its own engine: tile directory, deltas, statistics, manifest, slots and its own grid, a file of
extents only that replica's engine writes (T21). A grid shared by the three replicas of a core would interleave three
partitions' tiles differently on every node, and T6's identical grids would be lost. The core keeps one tile cache for
its replicas, since what a cache holds never changes what is written. Everything on the data plane
runs on the core's thread inside the partition's turn; other cores reach it only by messages. With one writer and its
readers on one thread, no data structure of the engine needs a lock or an atomic: a reader and the writer interleave
only between turns, and versions isolate them across turns.

The engine is a sans-I/O state machine. It consumes rounds to apply, read requests, I/O completions and the epoch
closes, and it emits answers and I/O requests (read a tile at an address, write an extent, sync, punch a hole). Its
compio shell performs the I/O; its simulator injects faults into the same interface, as the WAL's `sim_storage` and
`ultragraphdb_store_sim` do now.

### 4.2 Identity: slots

Adjacency and columns index by position, which is what makes GraphScope, Vineyard, Galaxybase and Kùzu fast. A
partition maps every entity it owns to a slot, a `u32` dense per (bucket group, type): slots are given in order of
arrival, never reused, and a deleted entity leaves a hole the tiles skip. The map from ID to slot is a record run per
(bucket group, type) of `(sequence, slot)`, sorted by sequence. A partition's own creations take rising sequences, so the
run grows by appending; other groups' entities take the sequences between, so it is sorted but not contiguous, and a
lookup is a binary search over its fences and within one 32 KiB run. The map back is a slot's sequence
and bucket in two columns. A slot exists only inside its partition: compute, the wire and the WAL carry system IDs, and
the engine translates once per entity a request touches.

Foreign neighbours stay system IDs, encoded per tile (*Adjacency tiles*); a local reverse list holds subject slots,
4 bytes where the KV layout holds 16.

### 4.3 Three tile shapes

| Shape | Maps | Holds |
|---|---|---|
| Adjacency block | a row key to a sorted multiset of neighbours with an optional payload | forward lists (row: a local slot), local reverse lists (row: an object's ID or a literal's encoding), value and fulltext indexes (row: a token), HNSW layers |
| Column chunk | a slot range to values | a type's attributes, statement attributes, vectors |
| Record run | a sorted key to a small fixed record | the term dictionary and key claims, `@unique` claims, slot maps, marks, referrer sets, count indexes, statistics, the manifest itself |

All three share a header, the base-plus-delta versioning, the checksums, the grid and compaction. A tile is bounded:
an adjacency block or record run at a target of 32 KiB and at most 64 KiB encoded, a column chunk at 4,096 slots.

**The header,** 64 bytes, every reserved byte zero: magic, format version, shape, the tenant run, bucket group and
relation or type it belongs to, never the partition, which a move changes, its key fences, its base version, entry count, flags (version columns present,
append-only, encoded sections), the XXH3-64 of each section, its conservation digest (*Safety*) and the zone maps of
its inline columns. A tile names what it is, so a misdirected read is detected even before its checksum is compared.

### 4.4 Adjacency tiles

A relation's adjacency in one direction is a sparse matrix, rows the row keys and columns the neighbours, cut into
blocks by (row, column) fences, so the tiles are the leaves of a B+-tree over edge keys. A small row lives whole in a
block beside its neighbours; a hub's row spans as many blocks as it needs, split by neighbour fences, so no tile grows
past its bound however large the degree. The directory of a relation, its fences and tile references, stays in memory:
a billion edges at 4,000 a tile is 250,000 entries, about 8 MB.

Inside a block the layout is columnar, after DCSR:

- the rows present, as slot deltas or sorted keys, with `u32` offsets into the entries;
- the neighbours, sorted by system ID, which orders them by type prefix, then sequence, then bucket: a run-length
  column of type prefixes (a type filter is a range), sequence deltas bit-packed in blocks of 128, the 24-bit buckets as
  a fixed column (they route the next hop);
- a parallel-edge index where the relation is `@multi`, and a statement slot where an instance carries a reifier;
- inline columns for small fixed-width edge attributes the catalog places there (a `knows.creationDate`);
- version columns only where *Versions* asks for them;
- a filter of its (row, neighbour) keys, so an insert proving a `@set` combination absent reads the block only when the
  filter answers maybe. Record runs carry the same filter over their keys for key claims and the dictionary.

A row ordered by an inline or denormalised attribute instead of by neighbour (*Schema-compiled layout*) keeps a sorted
prefix and an append tail, as GraphScope's `unsorted_since_` does: a top-k scan reads the tail backwards, then binary
searches the sorted prefix and stops at k.

### 4.5 Column chunks

A chunk holds 4,096 slots of one attribute: a fixed-width array with a validity bitmap; strings as `u32` offsets into a
data section, dictionary-encoded when the catalog's statistics show few distinct values, LZ4 only for a section above a
size threshold; a set-valued attribute as offsets into a value section. Values longer than a tile's bound go to blob
extents referenced by address, length and checksum. A single-valued role of a relation (a message's creator, `replyOf`)
is a column of neighbour IDs on its anchor, GraphScope's `SingleMutableCsr`: an O(1) read with no row directory.

### 4.6 Record runs

Sorted fixed records under fences, searched by binary search within a 32 KiB run. The term dictionary is two: a run keyed
by the IRI's 64-bit hash, then the key bytes, mapping to the system ID, a foreign entity's copied entry among them
(P24 rule 3); and the canonical key as a column of the type, the way back. A key
claim, a `@unique` claim, a mark and a referrer set are records in the partition that owns them today (P24 rules 3 and
5, `SCHEMA.md` sections 7.2 and 9.1). Statistics are records per (graph, predicate) and per round (*The write path*).

### 4.7 Schema-compiled layout

A catalog version compiles to a physical layout, versioned with it: which relations keep which directions, what each
row orders by, which attributes sit inline, which columns each type has and in which storage class each lands. The
compiler takes it from what UGS already declares; the facts and their effect:

| Declared | Physical consequence |
|---|---|
| Navigation and inverse names | Both directions kept; a role declared `-> _` keeps no reverse rows (as GraphScope's `ONLY_OUT`) |
| A single-valued role (`?`, `{0,1}`) on the anchor | A column on the anchor's type, no adjacency block |
| Relation attributes of fixed width, at most 16 bytes together | Inline in the neighbour entry; the rest in the statement columns |
| `@immutable` attribute of the local subject, `@index(range)` on it | Denormalised into the reverse entry and ordering the reverse row, so "the latest messages of a person" is a top-k scan |
| `@append_only` type, `@immutable` attribute | Storage class I (*Versions*) |
| `@bitemporal` | Content in class I; `asserted_end` a class M column written once |
| `@index(range)`, `@index(fulltext)`, `@index(interval)`, `@index(geo)` | An index relation in adjacency blocks, rows the tokens |
| `@key`, `@unique` | Claim records at the owner P24 names |
| Type prefixes | Columns and slots per type; a supertype a range of prefixes |

The compiler derives every row order from these declarations and nothing else (T16): a row orders by neighbour ID
unless an `@immutable` attribute with `@index(range)`, inline on the edge or of the local subject in a reverse row,
orders it, newest first for temporal types. Statistics never change a layout, so the layout is a function of the catalog
version, as every replica needs. Each compilation reports each relation's directions, row orders, inline attributes and
storage class in a layout record beside the coordination report (`SCHEMA.md` section 9.5), so the cost of a declaration
is visible when it is written. UGS gains no annotation for it.

Tenants without a schema, and undeclared attributes of `@open` types, take the generic layout: one adjacency relation
per (graph, predicate) holding encoded terms, with the subject-predicate and object-predicate runs the KV layout keeps.

### 4.8 Versions

Every committed write applies to deltas in commit order, so every delta row is sorted by version, and a snapshot at
version `v` is a prefix of each row. Nothing uncommitted ever reaches a delta: the commit step validates, the round is
committed, then applied.

The **horizon** `H` is the oldest version any reader may still ask for: the oldest pinned snapshot, the oldest open
read-write transaction (at most 10 s, the bound decided for them) and the MVCC retention window UGS states. Compaction
folds an entry into a base without versions once it lies at or below `H` for everyone. Two storage classes follow:

- **Class I, version-complete.** Instances of `@append_only` types with their attributes and keys, and `@bitemporal`
  content. Their entries are never updated or deleted outside a purge, so a base keeps each entry's creation epoch, delta-coded against
  the tile's base version (one to two bytes), and one base answers every epoch. Compaction keeps no old base for them,
  readers at any version filter one column, and a read that asks only for the latest state filters nothing.
- **Class M, mutable.** Every other relation and attribute, an `@immutable` attribute of a deletable instance among
  them: its value never changes, but the instance may go. Entries above `H` carry a creation and a deletion version, as
  LiveGraph's edge log does; at or below `H` they are folded and carry none. One base answers every version at or above
  `H`, so the engine keeps no chain of old bases either.

A read at `v` merges three sources per row: the base, its versioned entries filtered at `v`, and the delta prefix at
`v`. A pin is a count per epoch in a small array; `H` is its minimum.

### 4.9 The write path

1. The commit step takes every request queued for the partition at the start of a turn, validates them and builds one
   round. The round's logical record (P18) is the WAL record and the replication payload, unchanged.
2. The WAL appends the round and syncs it, one `fdatasync` for every commit of the turn, issued asynchronously so the
   turn goes on. In stage 8 "committed" means held by a quorum.
3. Apply, once committed, entirely in memory. For each statement: the subject's slot (assigned on first sight); one entry
   appended to the forward delta row and one to the reverse delta row, from a pool of 256 B blocks allocated at startup
   and an open-addressing index of rows sized from the budget, both the replica's own, so no other replica's load
   changes when this one compacts (T6); attribute writes to their column deltas; the round's
   statistics accumulator; the relation's forward and reverse digests. A first reference to a foreign entity writes its
   mark, as today; a row gaining its first entry for a predicate writes the subject-predicate or object-predicate record
   only in the generic layout.
4. Statistics close with the round: one record per (graph, predicate) the round touched, versioned by the round. Reads
   take a visibility watermark or a closed epoch, both round boundaries, where the counts are exact; a transaction
   beginning at an earlier version it names inside a round adds the round's versioned entries up to it.
5. Acknowledge once the round is durable (stage 5.5) or committed with its exposure bound (stage 8).

A relation thus writes two delta entries and a share of one statistics record per round, against 9.1 store versions.
When the delta pool passes its high watermark, the commit step admits no new requests until compaction frees blocks,
and a request waiting past its deadline is refused `Busy`: overload is an error a client retries, never an allocation.

### 4.10 The read path and storage lanes

UGS classifies every write into coordination-free, segmented and coordinated paths and every query by locality,
monotonicity and the mutability of what it reads (`SCHEMA.md` sections 9.2, 9.6; the distribution architecture's
*Query consistency levels*). The engine turns that classification into physical work through lanes. The frontend and
the planner attach a lane to each storage request; the engine trusts nothing it can check, so a lane the catalog does not
allow for the request's relations is refused.

| Lane | Taken by | What the engine does | What it skips |
|---|---|---|---|
| **L — latest** | Monotonic-fast reads; session reads of one partition's latest state; linearizable latest-state reads once past the read barrier (T28) | Answers within one turn from base and delta as applied, dropping deleted entries: monotonic-fast and session reads on any replica that has applied the session's position, linearizable ones on the primary once its next round commits by a quorum in its view | Pins, epoch waits, comparing entries against a pinned version |
| **S — snapshot** | Snapshot reads at a closed epoch `e`, as-of reads | Pins `e`, filters versions at `e`, may span turns, on any replica that has closed `e` | Read-set recording |
| **X — serializable** | Reads of an open read-write transaction | As S at the transaction's version, recording what it read | Recording stable reads |
| **A — append** | Writes whose every predicate the catalog marks blind (`SCHEMA.md` section 9.6: coordination-free, no key or `@unique` claim, no check reading beyond the instance), the writes a DirectWrite may carry | Joins the round without validation, its conflict keys still entered | Validation, the read phase |
| **V — validated** | Segmented and local writes with checks | The commit step's validation against the conflict history | — |
| **C — sequenced** | Coordinated writes (stage 8) | Executed at the sequencer's position, reads at that position | — |

Two rules make the lanes cheaper, not only labelled:

- **One request, one turn, one state.** A lane L request first makes the tiles it needs resident, yielding while they
  load, and then answers within one turn, while the thread runs nothing else, so it reads one consistent state with no
  pin. A tile evicted or replaced by compaction before that turn sends it back to loading, at most twice, after which it
  is answered in lane S. Its bound is counted, never timed
  (T17): at most `lane_entries_max` entries examined and `lane_bytes_max` bytes answered, both fixed at startup, so the
  same request does the same work on every replica and under the simulator. A result past either bound ends with a resume
  key naming a position by key, never by offset: the row key and the last neighbour's ID it answered. The next request
  seeks strictly past that key at the then latest state, so no entry is answered twice and none present at both times is
  skipped; an entry inserted behind the key since is missed, which monotonic-fast semantics allow and the response flags
  for mutable data.
- **Stable reads record nothing.** The existence of a class I instance, and its attributes and key, change only by a
  purge, so lane X leaves such a read out of the read set. A purge enters a conflict range over the instances it removes,
  which aborts every open transaction of the partition that could have read them. An `@immutable` attribute of a
  deletable instance is recorded: the value cannot change, but the instance can go. Absence, ranges and aggregates are
  always recorded: an insert can create a phantom.

A turn serves the commit batch first, then lane L, S and X requests up to a byte and entry budget each, then one
compaction beat, then submits its I/O; every budget is fixed at startup. A request needing a tile not in the cache
yields, its read issued in the turn's batch, and resumes in a later turn; up to a fixed number of such requests wait at
once, so the core keeps many reads in flight, as the io_uring paper's fibers do.

Compute sends frontiers, not vertices: one request names many sources of one relation and direction, with a type range,
inline-attribute filters and a top-k, and storage answers with the surviving neighbours in batches. A filter on a type
prefix is a range of the neighbour column; a filter on an inline attribute skips blocks by their zone maps; top-k reads
sorted rows from the end.

### 4.11 Compaction

A tile is rewritten when its delta reaches θ of its base (0.5 to start), when a delta row ages past a number of rounds,
when one delta row reaches `delta_row_entries_max` (1,024 to start, T19), which compacts a hub's row in the next beat
ahead of every other tile, or when the replica's pool passes its watermark. A block above 64 KiB splits at a row or
neighbour fence, and adjacent blocks below 8 KiB merge, so tile sizes stay bounded both ways. Galaxybase splits a
partition whose writes outgrow its reads; reads differ by replica, so here only a tile's size and its own rewrite count
decide where it splits.

**Paced.** Each applied round earns compaction work in proportion to its bytes, done in beats at the end of turns, so
compaction keeps pace without bursts and its CPU sits on the core that owns the data. No tile is compressed or encoded
on the commit path.

**Deterministic.** Which tile is rewritten, when, how it is cut and where it is placed depend only on the applied rounds:
counters of bytes and rounds, fences and a deterministic allocator, never the clock, reads, the cache's state or another
replica's load. A rewrite is decided, and its address allocated, at a round boundary; reading an uncached base tile may
delay when its bytes are written, never what they are or where. Replicas applying the same rounds therefore write the
same tiles at the same addresses. Every replica compacts for itself on the commit path, spending the same compaction
CPU, and the checkpoint identity of *Safety* checks that they agree (T29).

**Verified.** Compaction builds the new tile in a buffer from the pool, decodes it once and checks its order, counts and
digest against the merge of its inputs before it is written (*Safety*).

### 4.12 The grid, checkpoints and recovery

Each replica's grid is a file of extents of 1 MiB to start, preallocated with `fallocate` and written with `O_DIRECT`.
Tiles are packed into 4 KiB slots within an extent, so no tile straddles a 4 KiB boundary it does not start at; an
extent is written in 128 KiB to 512 KiB I/Os. A tile reference is (extent, offset, length, XXH3-64 of the tile).

**The manifest** maps every live tile to its reference, as record runs of its own, changed by an append-only manifest
log of additions and removals and compacted like any run. **The free set** is a bitmap of extents: an extent freed in
the interval after checkpoint `N` is reused only once checkpoint `N + 1` is durable, so recovery from `N` always finds
its tiles intact.

**A checkpoint,** of one replica, every `C` bytes of its WAL, and at a clean shutdown; each replica keeps its own
schedule, set by its log's positions alone (T21):

1. Write and sync the tiles compaction produced since the last checkpoint (most are already written; compaction writes
   continuously).
2. Write and sync the manifest log's new entries and the free set.
3. Write the superblock: four copies in the grid file's first four 4 KiB slots (T22), each checksummed, holding the checkpoint's
   sequence, the parent superblock's checksum, the manifest's root reference, the free set's reference, the WAL position
   the checkpoint covers and the replica's applied position. Sync between copies.
4. Trim the WAL through the checkpoint's position.

**Recovery:** read the superblock copies and take the newest whose checksum and parent chain hold; load the manifest
and the free set; rebuild the directories and slot maps from their runs; replay the WAL from the checkpoint's position,
which rebuilds the deltas and, compaction being deterministic, the same tiles. `C` bounds the replay: 1 GiB of WAL is
about 7 million relations at 150 B, an estimated 3 to 10 s of apply, where GraphScope's full-history replay took 552 s at
SF300.

### 4.13 Writing to the SSD

- **Out of place,** always: no tile is overwritten while any durable superblock can reach it, so a torn tile write
  harms nothing, and there is no doublewrite.
- **Compressed and packed:** lightweight encodings everywhere (bit-packing, frame of reference, run length, dictionary),
  which decode per row without decoding a tile; LZ4 for string sections and for tiles of the cold stream; tiles packed
  into 4 KiB slots.
- **Grouped by death time:** three write streams of extents per replica's grid, nine open extents a core, hot (tiles rewritten within the last few
  checkpoints), cold (tiles not rewritten for many) and manifest, a tile's stream chosen from its last four rewrite
  positions as the SSD paper estimates death time. Class I tiles of sealed rows settle in the cold stream by themselves.
- **Reclamation apart from compaction:** an extent with no live tile is freed and its blocks released with
  `fallocate(PUNCH_HOLE)`; when free space falls below 15% of the grid, a cleaner copies the live tiles of the extents
  with the fewest live bytes into the cold stream. Cleaning is paced and deterministic like compaction.
- **The WAL in its own files,** never mixed with tiles: its segments die together at a trim.
- **Device placement later:** zones sized to the drive's reclamation unit, FDP placement identifiers and NVMe passthrough
  need physical Linux drives and are stage 10 experiments; the extent stream is the interface they replace.

**Without power-loss protection,** the default: a durability point is an `fdatasync` that reaches the drive's flush, after
the writes it covers have completed. The flag tunes batching and round bounds, never a sync. Every tile, manifest entry,
superblock copy and WAL frame is checksummed, so a write torn beyond 4 KiB is detected and, being out of place,
unreferenced.

### 4.14 Larger than memory

Each core's tile cache is a fixed set of frames allocated at startup, in size classes of 4 KiB multiples up to 64 KiB,
indexed by a table from tile reference to frame. Cached tiles stay encoded; a read decodes the rows it touches. A clock
sweep evicts; a frame read by a request waiting on I/O is pinned until that request's turn ends. Base tiles are clean, so
eviction never writes. Reads are batched per turn and capped, since a batch of 128 writes measured 201 µs against
11.5 µs alone; a frontier request prefetches the blocks its sources fall in.

The io_uring settings the paper supports, each a measured experiment here: `single_issuer` and `defer_taskrun` on every
core's runtime, which compio exposes today (`ProactorBuilder::single_issuer` and `defer_taskrun`, set through
`RuntimeBuilder::with_proactor`), so `core_runtime` sets them with no change to compio; registered buffers over the frame
pool and the extent write buffers, which compio-driver 0.12.5 lacks (no `register_buffers`, no fixed-buffer read or
write) and which a custom operation could add only through `unsafe impl OpCode`, refused by the workspace (T18); one ring
per core for storage and network; no SQPOLL under one partition per core; polled completion on a second ring and NVMe
passthrough as later options, both requiring raw devices or a compatible file system; I/O sizes and in-flight counts read
from the device's `max_hw_sectors_kb`, `max_segments` and `nr_requests` at startup and clamped to them, so nothing falls
back to kernel workers.

### 4.15 Safety

**Fault model.** Crashes at any instruction; torn writes beyond 4 KiB; lost writes the drive acknowledged; misdirected
writes and reads; latent sector errors and bit rot; a failed `fdatasync`, after which no retry is trusted.

**Detection.** Every tile carries its section checksums and is named in its header; every reference to a tile carries
the tile's checksum, so a stale or misdirected tile fails the comparison even when its own checksum holds; the
superblock's copies are chained by their parents' checksums; the WAL's rounds are chained as today.

**Conservation invariants,** checked while running, each a returned invariant error under Ultra Style rule 5:

- Every tile carries a digest of its entries, the wrapping `u128` sum of XXH3-128 over each entry's logical bytes,
  independent of encoding and order (T20). A rewrite must satisfy `digest(new) = digest(old) + digest(inserted) − digest(deleted)`, checked
  before the tile is written; a compaction bug fails here, not in a reader months later.
- Each relation keeps a forward and a reverse digest over its (subject, object) pairs, updated by apply, equal after
  every round. P24 keeps both sides in one partition, so the background verifier of the distribution architecture
  becomes an O(1) check per round.
- Decoding a tile checks every offset, count and fence before handing out a view, as the wire's frames are checked; every
  decoder of bytes from disk has a fuzz target.

**Response.** A tile failing its checks is fetched from a replica, whose grid holds the same bytes at the same address
(T6), or rebuilt by replaying the WAL when the WAL still covers it. A replica that cannot repair stops serving and says
which tile failed; it never answers from a damaged tile. A failed sync stops the node, as today.

**Checkpoint identity.** Each checkpoint's identity is the XXH3-128 of its superblock's checkpoint state, which commits
to the manifest and its digests. It rides on `Prepare`, `PrepareOk` and the commit heartbeat; a replica receiving
another identity for a checkpoint it holds stops with an invariant error naming both (T29). Replicas compute their
checkpoints independently, so a fault in one replica's memory or disk shows as a mismatch instead of spreading.

**Scrubbing.** A paced background pass reads every tile of the grid once per cycle and verifies its checksums and
digest.

**Limits.** Every pool, directory, cache, queue and batch has a capacity fixed at startup, from the node's memory budget;
every loop over tiles, rows or I/O counts against a named bound.

**Simulation.** The engine runs under the deterministic simulator with every fault class above, against a shadow model:
a plain map of quads at every version. The simulator also checks that two replicas fed the same rounds produce
byte-identical grids.

### 4.16 Distribution

**Placement stays P24's.** An entity's facts live in its bucket's partition, an edge's forward and reverse entries in its
subject's partition, and a reverse read fans out through the object's referrer set. GraphScope's Groot, Vineyard and
Galaxybase store an edge at both endpoints' owners instead, which makes every edge between partitions a two-owner write;
P24 rejected that on 2026-11-02 and nothing here reopens it.

**The bucket group is the physical shard.** A tile holds rows of one bucket group, and slots are numbered per (bucket
group, type), so GSL's levels become G the replica group, S the bucket group (the unit P24's referrer sets already
name), L the tile. A move of a bucket group is a logged operation at a position of the source's log: there the source
compacts the group's deltas into tiles, deterministically, and ships them with the group's slot maps; the destination
installs them at a position of its own log, copying the bytes unchanged into its grid, since no header names a
partition. Nothing is re-cut or renumbered. A tenant with little data pays with small tiles, which pack into 4 KiB slots.

**Replicas catch up by tiles.** A follower within one checkpoint of the primary repairs its WAL and compacts as usual. One
more than a checkpoint behind state-syncs lazily, as TigerBeetle's replicas do: it takes the primary's latest superblock
first, fetches the tiles it references on demand by address and checksum, identical to what it would have built, then
replays the WAL above it (T29).

**Analytics read projections.** Whole-graph algorithms want the two-dimensional partitioning the earlier tile note
proposed, with about √p peers a step. Compute builds it as an immutable projection at a closed epoch, assembled from tiles
streamed from storage and cached on compute, as GRAPE builds its fragments from Vineyard; the store itself keeps the
one-dimensional placement that serves transactions.

**S3 mode** follows from the format: tiles are immutable, checksummed and self-describing, so a checkpoint's tiles can be
uploaded and cached by compute keyed by reference.

**Placement by affinity is measured before it is built** (T15). Today "the latest messages of a person" is a reverse
read fanning out to every partition holding a referrer group of that person, since a message's bucket comes from its
own key. Frontier requests bound that to one request per partition per hop, whatever the number of sources, and a
multi-source hop (IC2's friends' messages) reaches most partitions under any placement, since the friends themselves
are spread. Affinity would help single-source reads (IS2). Its design, if E11 asks for it: an entity type takes its
bucket from a required single-valued participant, its key's claim and IRI entry stay at the key's hash owner as a
get-or-create registration (P24 rule 3's mechanism), and a lookup by key costs one more hop.

### 4.17 Writes through the protocol: the frame as the record

**What the question was.** Could an insert's frame body take a byte layout the store processes directly, the WAL write
the message as received, and the copies and I/O between the wire and the tiles shrink further?

**What holds today.** A DirectWrite carries a term table (24-byte records over a text section, terms as `u32` indices)
and 16-byte change records over `u32` args (`PROTOCOL.md` sections 4.4, 4.6, 6.6). The partition resolves terms to
system IDs and re-encodes every statement as a key-value entry of the write-set record (section 12.2), which the Round
frame logs, stage 8 replicates and replay decodes.

**What works.** An insert frame whose body is the logical record once its terms are resolved:

- A new command, **Ingest**, carries blocks. A relation block names one (graph, predicate) and holds, after DCSR: its
  rows (subject term indices, each subject once), `u32` offsets into its entries, the neighbours (term indices), and its
  inline attribute columns at fixed width in the store's value encoding (a `dateTime` as an `i64`), each with a validity
  bitmap where optional. An entity block names one type and holds its key column and attribute columns the same way. The
  command header names the catalog version the client encoded against; another version in effect refuses the frame
  naming it, and the client describes again.
- The partition writes the body to its log unchanged, followed by a resolution section: per term index the system ID it
  resolved to, a statement ID per edge that takes one, and the round's dictionary entries as now. The record stays
  logical, holds no store key and replays to the same state: a new Round entry kind beside section 12.2's three. For the
  body to reach the disk without a copy, the Round frame places it at a 4 KiB-aligned offset, padded with zeros, a change
  to section 12.1's layout. A body logs its terms' text where the write-set record logs 16-byte IDs; a batch interns each
  term once, and E12 measures which is smaller.
- Apply walks the blocks in place: per block one catalog lookup, one rights check, one lane decision and one check per
  column (a datatype and a value check over a whole column, as a loop over a slice); per row one slot and one delta row
  probe for its whole run of entries; per entry one reverse append. The row probe is paid once per subject, not once per
  edge, which a record of quads cannot offer.

**Copies,** per byte of an Ingest body at the partition:

| Step | DirectWrite today | Ingest |
|---|---|---|
| Receive to log | parsed, resolved, re-encoded as write-set entries | written as received: a body in a 4 KiB-aligned pool buffer, padded to 4 KiB, goes to the log by a vectored `O_DIRECT` write with no copy; a body below 4 KiB is copied into the round's buffer, about 100 ns |
| Log to replicas (stage 8) | the Round's bytes | the same bytes, sent zero-copy above 1 KiB |
| Apply | decode the record, then 9.1 keys a relation | read in place, two delta entries a relation |
| Deltas to tiles | — | compaction, as every write |

The one copy left on the commit path is the one the store's shape requires: deltas grouped by row, across commits, in
both directions, in slots and versions.

**What does not work.** Making a frame's block the tile's bytes, or having deltas point into retained frame buffers.
A tile orders rows by slot and neighbours by ID across many commits, holds versions, keeps the reverse direction grouped
by object, and lives in the partition's ID space; a frame is one client's batch, in arrival order and term space.
Referencing frame buffers from deltas would hold every receive buffer until the last row it touched compacts, so memory
would be bounded by the slowest row instead of by the pool, and it would save a copy of about 48 B a relation, 10–20 ns,
against an apply estimated at 0.3–1 µs. The gain is the record's encoding and the per-row work, not this copy.

**Why a batch is the fastest insert.** Every fixed cost of a write is paid per frame or per block in place of per
statement: the session and rights checks, the catalog lookups, the lane decision, the validation's setup, the round's
sync and the frame's checksum. TigerBeetle amortises its consensus the same way, up to 8,190 events a request. An Ingest
block of class I data with checks only over the instance takes lane A: no read phase, no validation.

**Partitions.** A frame's subjects usually span partitions. The frontend already finds each keyed IRI's bucket from the
catalog (`PROTOCOL.md` section 11.12); it splits an Ingest into one share per partition, copying each block's rows once,
and relays each share as a frame of its own. Each share commits atomically at its partition (T23, T24).

### 4.18 Client-generated identifiers

TigerBeetle's time-based identifiers put a millisecond timestamp in the high 48 bits and 80 random bits below, and
increment the random bits for further IDs within one millisecond, so one generator's IDs rise strictly; its
documentation recommends them as idempotency keys that need no central oracle and keep long increasing runs, which an
LSM tree turns into throughput. They order IDs per generator, not across clients: the documentation claims no cluster-wide
total order.

What each property is worth here:

- **A total order of transactions** is the commit version within a partition and the closed epoch across partitions,
  decided by the commit step and the sequencer. A client's ID can record the order of its own intents, never the
  database's order.
- **Locality** is already the server's: system IDs order by type prefix and sequence, sequences leased in rising blocks,
  so a partition's new entities append. A client-chosen system ID would also break P24's bucket and the rule that system
  IDs never leave the database.
- **Idempotency across an application's crash** is the gap. The session table makes a commit exactly-once while its
  session lives; an application that crashes and starts a new session retries as a new request. A keyed entity and a
  `@set` relation are already idempotent by their key or combination; a `@multi` relation instance and a keyless event
  are not.

T26 closes that gap with a key type and leaves order and locality where they are.

### 4.19 Networking between roles

The bi-channel paradigm (Kreuzmayr, El-Hindi, Wagner, Ziegler, Leis; EDBT 2027) splits a cluster's traffic: control
messages over kernel TCP, bulk data as user-space UDP over DPDK on a second NIC, retransmitted over TCP when lost. On AWS
c7gn (200 Gbps) kernel TCP over io_uring moved 5.83 Gbps per core against DPDK's 190 (Fig. 4), cost 10,320 cycles per
64-byte message against 41 (Fig. 5), and a replicated key-value store reached 3.9 million updates a second against
TCP's 1.5 million, its min-to-p99 spread 9 µs against about 140 (Figs. 17, 18). Its TCP baseline is this repository's
stack: io_uring, a ring per thread, zero-copy send.

Its gains sit where messages are small, unbatched and at tens of millions a second. UltraGraphDB batches Prepares into
rounds and reads into frontier requests, keeps epoch closes and registrations at hundreds a second, and moves tiles in
32–64 KiB units. Adopting it would bring DPDK or AF_XDP, both foreign memory rings reached through `unsafe`, a poll-mode
loop beside compio's, a second NIC per node through SR-IOV in Kubernetes, and a data path outside TLS and network
policy. Four of its lessons hold for any transport and cost nothing; T27 takes them.

## 5. What the engine asks of the existing code

| Today | With the engine |
|---|---|
| `ultragraphdb_traits_store::Store`, Badger's versioned KV surface | Retired. The engine's interface works at the access-pattern level: apply a round, read frontiers, columns and records at a lane, step compaction and checkpoints |
| `ultragraphdb_store_kv` (fjall, moka), `ultragraphdb_store_posting`, the keyspaces of `ultragraphdb_store_quads` and `ultragraphdb_core_keys` | Replaced by tile crates: a pure format crate (encodings, headers, digests, no I/O), the engine's state machine, the grid (extents, free set, manifest, superblock) with its compio shell, and its simulator |
| `ultragraphdb_store_apply` writing keys | Writing delta entries from the same changesets; the checks against the catalog stay |
| `ultragraphdb_store_hnsw` over the vector keyspace | HNSW layers as adjacency blocks over slots, vectors as column chunks (stage 8, as staged) |
| Checkpoints through `FjallStore::sync` | The grid's checkpoint, then the WAL's trim |
| A partition taking one request per turn from each role connection | Turns that take every queued commit into one round, and serve lanes with budgets |
| Compute reading quad patterns per key | Frontier requests with type ranges, inline filters and top-k |
| Statistics as deltas per commit | Records per round |
| The compio runtime built with defaults (`core_runtime` in `ultragraphdb_common_service`) | `single_issuer` and `defer_taskrun` set through `RuntimeBuilder::with_proactor`, I/O sizes clamped to the device; compio itself unchanged (T18) |
| DirectWrite's changes re-encoded as a write-set record | Ingest, a block-columnar insert command, its body logged as received with a resolution section (*Writes through the protocol*) |
| Sequences leased per (type, partition) | Kept: slots numbered per (bucket group, type) give the dense positions a lease per group would, with no more leases |
| Conflict keys naming list keys | Kept logical: (graph, predicate, subject) and the like, independent of tiles, so a split never changes a conflict |

The commit step, the WAL and its record, the sequencer, the metadata group, referrer sets and the wire protocol keep
their shape. A storage differential test runs the same generated workloads on both engines until the KV store is
removed, as the earlier tile note proposed.

## 6. Decisions

Each states the trade-off or the principle, and the choice. All were adopted on 2026-11-10: T1–T14 as the draft
recommended them, T15–T29 as the answers to its open questions, each argued from TigerStyle's principles.

**T1. The engine is graph-native and whole.** *Trade-off:* keeping fjall as a write layer under read tiles (the
discarded hybrid) keeps the 9.1 records and the LSM's rewrites a relation pays; a graph engine over a KV interface keeps
the records. *Choice:* replace the KV store and its trait.

**T2. Three tile shapes, one versioning scheme.** *Trade-off:* one shape per kind of index fits each better and multiplies
the code that must be proved, fuzzed and simulated. *Choice:* adjacency blocks, column chunks and record runs,
sharing header, versions, checksums and compaction.

**T3. Slots per (bucket group, type).** *Trade-off:* addressing by system ID needs no map but makes every attribute
access a search and every local reverse entry 16 bytes; slots cost one translation per entity a request touches.
*Choice:* slots, never leaving the partition.

**T4. The bucket group is the physical shard.** *Trade-off:* tiles spanning groups pack small tenants better; tiles
within a group make a move a copy and match the referrer sets. *Choice:* tiles within one group.

**T5. Memory-only apply; tiles out of place; the superblock last.** *Trade-off:* writing tiles in place saves the
manifest and free set and costs doublewrites and torn pages. *Choice:* as described, after TigerBeetle.

**T6. Deterministic compaction and placement.** *Trade-off:* adaptive compaction reacting to reads or the clock could
fit a workload better; determinism gives identical grids, peer repair, catch-up by copying and a simulator that compares
replicas. *Choice:* determinism.

**T7. The catalog compiles the physical layout.** *Trade-off:* a generic quad layout serves every tenant identically; a
compiled one serves declared tenants several times faster and needs a layout version beside each catalog version.
*Choice:* compiled for governed tenants, generic for the rest.

**T8. Version-complete tiles, in two storage classes.** *Trade-off:* chains of old bases are simpler to build and keep
old tiles alive in cache and on disk; per-entry versions above the horizon cost a column. *Choice:* classes I and
M as described.

**T9. Storage lanes from the classification.** *Trade-off:* one read path for every request is simpler; lanes let the
schema's declarations remove pins, filters and read-set entries. *Choice:* lanes L, S, X for reads and A, V, C
for writes.

**T10. Shared-nothing per core.** *Trade-off:* sharing immutable tiles across cores would serve one partition's reads
from several cores, and needs reference counting across threads and a shared cache. *Choice:* one core per
replica's engine; reads scale through the three replicas of every partition and through compute.

**T11. Conservation digests as running invariants.** *Trade-off:* a few nanoseconds per entry. *Choice:* adopt.

**T12. Analytics through compute-side projections.** *Trade-off:* two-dimensional placement in storage serves analytics
and costs every short read √p partitions. *Choice:* projections at a closed epoch.

**T13. SSD-aware writing** as in *Writing to the SSD*. *Choice:* adopt the stream, packing and reclamation rules
now, device placement in stage 10.

**T14. The KV store's crates and trait are retired** once the differential test passes on every suite.

**T15. Placement stays P24's; affinity is measured first.** *Principle:* sketch before building, and do it right the
first time: frontier batching bounds a hop to one request per partition, and affinity helps single-source reads only.
*Choice:* no change to placement now; E11 measures the share of short reads touching more than one partition, and the
affinity design of *Distribution* is adopted only on that evidence.

**T16. Row orders come from declarations alone.** *Principle:* explicit, deterministic, minimal surface: no annotation
UGS lacks, no layout drawn from statistics, which differ by replica and time. *Choice:* neighbour ID order, or an
`@immutable` `@index(range)` attribute's order, newest first for temporal types; a layout record reports it.

**T17. Lane L is bounded by counts, its resume by key.** *Principle:* put a limit on everything, and never react to
time: a count gives the same work on every replica and in the simulator. *Choice:* `lane_entries_max` and
`lane_bytes_max`, fixed at startup; resume keys name the last row and neighbour.

**T18. No compio fork.** *Principle:* zero technical debt and few dependencies: a fork is a dependency maintained alone.
*Choice:* `single_issuer` and `defer_taskrun` are set now through compio's public builder; registered buffers wait for
E9 on physical Linux, and only a measured gain sends a patch upstream, since a custom operation in this workspace would
need `unsafe`. A fork exists only to pin an upstream change accepted but not yet released.

**T19. A hub's delta row is capped.** *Principle:* bound every queue; one pool per replica keeps one budget to reason
about, and no hub takes a pool of its own.
*Choice:* `delta_row_entries_max`, 1,024 to start; a row reaching it compacts in the next beat, ahead of other tiles.

**T20. Digests are 128 bits.** *Principle:* safety first when the cost is nanoseconds; a digest is the last line
against a silent compaction bug, compared across billions of entries and replicas. *Choice:* the wrapping `u128` sum of
XXH3-128 per entry; section checksums stay XXH3-64, as the wire's.

**T21. One grid and one checkpoint schedule per hosted replica.** *Principle:* determinism decides layout: a grid shared
by a core's replicas differs on every node and breaks T6. *Choice:* each replica writes its own grid file and
checkpoints at its own log positions; the core shares only its tile cache.

**T22. The superblock lives in its grid.** *Principle:* fewer files, fewer failure modes: a superblock in a separate file
can go missing while its grid stays. *Choice:* four copies in the grid file's first four 4 KiB slots.

**T23. An Ingest frame is atomic per partition share.** *Principle:* bound the work and make the outcome explicit; one
failure model per share is simpler than a partial-failure model per frame. *Choice:* the frontend splits a frame into
shares; each share is atomic at its partition and exactly-once under the session table's key (session, request,
share), so a resend commits only the shares not yet committed and answers the rest from the table. The reply lists every
share's outcome, a refused share naming its first failing block and row. From stage 8 a flag asks for the whole frame
sequenced at one epoch, Calvin's path.

**T24. Ingest rows succeed or fail with their share.** *Principle:* minimise dimensionality: per-row results and a chain
flag would add what the share already gives. A graph insert is rarely independent: an edge needs its endpoint in the same
batch. *Choice:* no per-row outcome beyond the first failing row a refusal names.

**T25. Ingest alone logs its frame as received.** *Principle:* two record formats, each exact, beat one format with
optional resolution. *Choice:* a Write or a DirectWrite holding deletes by pattern, operations or statement properties
keeps the write-set record, which holds what execution resolved; inserts move to Ingest on the client side.

**T26. TigerBeetle's time-based identifiers become a key type, never system IDs.** *Principle:* close the gap there is,
idempotency, and leave order and locality where the server already has them; system IDs never leave the database.
*Choice:* a UGS scalar `Id128` with TigerBeetle's layout (48 bits of milliseconds, 80 random bits incremented within a
millisecond, compared as an unsigned integer), declared as the `@key` of `@multi` relations and of event-like
`@append_only` types, so an application retrying after its own crash finds the instance it created; an `id()` generator
in every client; the key's claim run ordered by the key itself for this type, so claims append. The session table stays
the transport's exactly-once mechanism, and the commit version and epoch stay the database's order.

**T27. No bi-channel transport in stages 8 and 9; its transport-free lessons now.** *Principle:* measure before adding a
subsystem; a dependency reached through `unsafe` needs evidence and consent. *Choice:* several connections per peer for
any stream above about 8 Gbps, since AWS caps a flow at 10 Gbps (the paper's §4.2); in-flight limits through credits
sized to the receiver's buffers (Guideline 4); VSR's acknowledgements apart from flow-control credits (§6.2); clients on
TCP. A prototype follows in stage 10 only if E13 measures networking above 30% of CPU, over AF_XDP, for compute–storage
frontier traffic first.

**T28. Latest-state reads rest on quorums, never on clocks.** *Principle:* safety never depends on time; batch the
control plane. *Choice:* session reads on any replica that has applied the session's token; linearizable latest-state
reads behind a read barrier batched per turn, answered once the primary's next round commits by a quorum in its view; an
empty round on an idle partition; no leases (section 8).

**T29. Every replica compacts for itself, and checkpoints carry an identity.** *Principle:* the scarcer resource pays
least, and independent computation detects faults. *Choice:* followers compact on the commit path as the leader does;
tiles move between replicas only for repair and for a replica more than one checkpoint behind, which state-syncs lazily;
the XXH3-128 of each checkpoint's state rides on `Prepare`, `PrepareOk` and the commit heartbeat, and a mismatch stops
the replica with an invariant error (section 8).

## 7. Experiments

| # | Question | How | What it decides |
|---|---|---|---|
| E1 | What does apply cost per relation? | `wal_bench` against the tile engine at 1, 10 and 50 relations a commit, profiled as the baseline's commit thread was | Whether T1's target holds |
| E2 | Which tile size? | 16, 32 and 64 KiB targets: one-hop latency cached and cold, rewrite bytes, cache efficiency | The bound of every shape |
| E3 | Which θ? | 0.25, 0.5, 1.0 under the LDBC insert mix: bytes written, delta memory, CPU | The rewrite factor |
| E4 | Which encodings? | Bit-packing, frame of reference, LZ4 per section on LDBC SF10 and DeepBrain data: size and decode ns per entry | The format's sections |
| E5 | Do lanes pay? | The same reads in lanes L, S, X: CPU, latency, read-set sizes | T9 |
| E6 | Is compaction deterministic? | Two simulated replicas, every fault class, grids compared byte for byte at every checkpoint | T6 |
| E7 | How does the engine compare to the state of the art? | LDBC SNB Interactive v1 and v2, official driver, SF10 to SF300, every commit synced, per physical core | The stage's exit |
| E14 | How often does an insert read a tile to prove an absence? | The LDBC insert mix and DeepBrain's writes: filter answers of maybe, tile reads and their I/O on the commit path, at filter sizes of 8 and 12 bits a key | The filters' size |
| E8 | What does the drive see? | Physical Linux, drives with and without power-loss protection: host bytes against the drive's physical writes, WAL included | T13 and the stream count |
| E9 | Which io_uring settings pay? | Each setting of *Larger than memory* in isolation on physical Linux | The runtime's defaults |
| E10 | How long is recovery? | Kill and power-cut at `C` = 256 MiB, 1 GiB and 4 GiB a partition | `C`'s default |
| E11 | How many short reads cross partitions? | E7's run, each IS and IC query counted by the partitions it touches, at 6, 12 and 32 partitions | T15: whether affinity is built |
| E12 | What does Ingest save? | The LDBC update stream as DirectWrites and as Ingest frames of 1, 64 and 1,024 rows: log bytes, CPU per relation from receive to apply, copies counted | Ingest's block layout |
| E13 | How much CPU does networking take? | Cycles per Prepare and per frontier message on compio TCP with real batching, at the cluster's rates, on physical Linux | Whether a bi-channel prototype is built (gate: above 30% of CPU) |

## 8. Reads after a view change, and compaction on followers

The draft's last two questions, investigated against TigerBeetle's internals (`docs/internals/vsr.md`, `lsm.md`,
`sync.md`, read 2026-11-10) and TigerStyle; T28 and T29 adopt the answers.

**How does a lane L read of one partition's latest state avoid a deposed primary in stage 8?**

*The problem.* A primary a view change has replaced does not know it until a message tells it. If it answers a
latest-state read from its own applied state, a client can read state older than a commit another client has already
been acknowledged by the new primary: a stale read, which strict serializability forbids. The distribution protocol leaves
this open for every latest-state read.

*What TigerBeetle does.* Its VSR document converts every client request into a prepare that is given an op and
replicated ("The primary converts the command=request to a command=prepare"), and it does not set reads apart. It
attributes ordering to consensus alone ("Viewstamped Replication to guarantee strict serializability for all
operations"). Its cluster clock serves timestamps and timeouts, and no page mentions leader leases. Its sessions
guarantee that a client "reads its own writes" and "observes writes in the order that they occur on the cluster", and
that replies to different sessions may arrive out of order. The documents read do not state outright whether lookups are
ordered through the log; that they are is inferred from the generic request path.

*The options.*

| Option | Safety rests on | Cost |
|---|---|---|
| Leader lease | Bounded clock drift between replicas | No messages; a wrong clock serves stale reads silently |
| One confirmation round per read | Quorum intersection | A quorum round trip per read, which spends what lane L saves |
| A read barrier batched per turn | Quorum intersection | One quorum round trip of latency per turn's reads, usually zero added messages |
| Session reads only | The session token | None, but no guarantee across sessions |

*Decision (T28).* Two read levels, neither depending on a clock:

- **Session reads**, the common case. A read naming its session's token is answered by any replica that has applied the
  token's position, with no confirmation: it sees its own writes and never goes back in time, which is TigerBeetle's
  session guarantee. Monotonic-fast reads stay as they are.
- **Linearizable latest-state reads** pass a read barrier. The primary holds every such read that arrives during a turn
  behind the next round it prepares, and answers them, in one later turn, once that round is committed by a quorum in the
  primary's view, at the state applied through it. A quorum's `PrepareOk` in the current view proves the primary was
  primary when the round committed, by the intersection of quorums, so a deposed primary cannot gather it and redirects
  the reads instead. Under write load the round is in flight anyway and the barrier adds no message; on an idle partition
  the primary prepares an empty round, one header and no entries, which the existing protocol and its Lean model (X3)
  already cover, so no new message type is needed.

*Why, by TigerBeetle's principles.* Safety never rests on a clock: leases are rejected for the reason the distribution
architecture rejected hybrid logical clocks. The barrier reuses the replication path instead of adding a protocol, which
keeps the interface small. Batching per turn turns a round per read into a round per turn, the control plane amortised
over the data plane. The price is explicit: a linearizable latest-state read waits about one commit's latency, so lane
L's "no wait" holds for session reads and monotonic-fast reads, not for this level.

**Do followers compact for themselves, or install the leader's tiles?**

*What TigerBeetle does.* Compaction runs on the replicated commit path: "A compaction tick executes asynchronously
immediately after every commit, with beat = commit.op % lsm_compaction_ops", so every replica compacts, paced by op
number. Determinism is checked rather than assumed: a checkpoint id is "a hash over the entire state", carried on
`commit`, `ping`, `prepare` and `prepare_ok`, and "If non-determinism is detected (via checkpoint id mismatches) the
replica which detects the mismatch will panic." Blocks travel between replicas only to repair a corrupt or missing
block, by address and checksum (grid repair), or when a replica is more than one checkpoint behind: then it state-syncs
lazily, taking the superblock first and the blocks on demand (`sync.md`). The documents give no rationale for this choice.

*The envelope.* Compaction is estimated at 0.2–0.6 µs of CPU a relation per replica (*Targets and the envelope*), so a
follower compacting for itself spends about one core-second for every 2–5 million relations. Installing the leader's
tiles instead sends a follower the tiles' bytes, about 50 B a relation by the same estimate, a third more than the
150 B of WAL it already receives, over the network, which the write-path note's tables show binding before the cores
from 48 cores a node. It would also need a second normal-path protocol, tiles ordered against rounds, and a rule for
when a follower drops the deltas a tile covers.

*Decision (T29).* Followers compact for themselves, as TigerBeetle's replicas do. Tiles move between replicas only for
repair and for a replica more than one checkpoint behind, which state-syncs lazily as TigerBeetle does. Each checkpoint
gets an identity, the XXH3-128 of its superblock's checkpoint state, which commits to the manifest and its digests; it
rides on `Prepare`, `PrepareOk` and the commit heartbeat, and a replica receiving a different identity for the same
checkpoint stops with an invariant error naming both, Ultra Style rule 5's fail-stop.

*Why, by TigerBeetle's principles.* The scarcer resource pays least: CPU per relation is small and stays on the core that
owns the data, while the network carries only the log. Independent computation is what makes the comparison a fault
detector: a bit flipped in one replica's memory produces a different checkpoint identity, where installing the leader's
tiles would copy the flip to every follower. One path, the commit path, builds every replica's state, so the simulator
checks one mechanism. If E1 measures compaction far above its estimate, the choice is revisited with the figure.

## Later

- Zone-sized extents, FDP placement and NVMe passthrough, on physical Linux drives in stage 10.
- Polled completion on a second ring per core.
- Tiles in S3 mode, cached on compute.
- Analytical projections and their two-dimensional placement on compute.
- Registered buffers, upstream in compio, if E9 measures a gain.
- A bi-channel data path over AF_XDP, if E13 measures networking above 30% of CPU.
- Placement by affinity, if E11 asks for it.

## Sources

Read for this draft:

- GraphScope, `~/RustroverProjects/fork/GraphScope` at `d8581c18`: `flex/storages/rt_mutable_graph`, `flex/utils`,
  `flex/engines/graph_db/database`, `interactive_engine/executor/store/groot`; the 2024 full disclosure report; the LDBC
  SNB Interactive audited results at ldbcouncil.org/benchmarks/snb/interactive/.
- Vineyard, `~/RustroverProjects/fork/v6d` at `0cbaada9`: `src/server/memory`, `src/server/services/meta_service.cc`,
  `modules/graph` (fragment, vertex maps, loader, partitioner).
- B. Tong et al., *Galaxybase: A High Performance Native Distributed Graph Database for HTAP*, PVLDB 17(12): 3893–3905,
  2024 (`papers/GalaxyGraph-p3893-tong.pdf`).
- M. Jasny, M. El-Hindi, T. Ziegler, V. Leis, C. Binnig, *High-Performance DBMSs with io_uring: When and How to Use It*,
  PVLDB 19(9): 2317–2330, 2026 (`papers/High-Performance DBMSs with io_uring_2512.04859v3.pdf`).
- B. Lee, T. Ziegler, V. Leis, *How to Write to SSDs*, PVLDB 2026, arXiv 2603.09927v3
  (`papers/How to Write to SSD_2603.09927v3.pdf`).
- TigerStyle, tigerstyle.dev; `openspec/protocols/TIGER_STYLE.md`, `ULTRA_STYLE.md`.
- G. Kreuzmayr, M. El-Hindi, B. Wagner, T. Ziegler, V. Leis, *The Bi-Channel Networking Paradigm*, EDBT 2027, arXiv
  2606.19969v1 (`papers/The Bi-Channel Networking Paradigm-2606.19969v1.pdf`).
- TigerBeetle, *Data Modeling*, "TigerBeetle Time-Based Identifiers",
  docs.tigerbeetle.com/coding/data-modeling/#tigerbeetle-time-based-identifiers-recommended.
- TigerBeetle internals, `docs/internals/vsr.md`, `lsm.md` and `sync.md` in github.com/tigerbeetle/tigerbeetle (main,
  read 2026-11-10); docs.tigerbeetle.com/coding/time/ and /reference/sessions/.
- compio-driver 0.12.5 and compio-runtime 0.12.6 as `Cargo.lock` pins them: `ProactorBuilder` (`single_issuer`,
  `defer_taskrun`), `RuntimeBuilder::with_proactor`, the `unsafe trait OpCode`.
- `specification/binary_protocol/PROTOCOL.md` sections 4.4, 4.6, 6.6, 11.12, 12.1 and 12.2.
- `openspec/benches/benches-2026-11-09.md`; `openspec/notes/backlog.md` items 18 and 19;
  `openspec/designs/UltraGraphDB Shard-per-Core Write Path.md` (P8–P11, P18, P24);
  `openspec/designs/UltraGraphDB Distribution Architecture.md`; `openspec/designs/UltraGraphDB Distribution Protocol.md`;
  `openspec/notes/UltraGraphDB Roadmap Dual Query Engine and Tile Storage.md`, the discarded hybrid;
  `openspec/notes/UltraGraphDB Schema and Query Language.md`; `specification/schema/SCHEMA.md` sections 9 and 12.

Named from memory, to be read before specification: TigerBeetle's storage internals (grid, free set, manifest log,
compaction pacing); LiveGraph (VLDB 2020) for per-entry creation and deletion versions; Sortledton (VLDB 2022) and Teseo
(VLDB 2021) for blocked sorted adjacency; LLAMA (ICDE 2015) for multiversioned CSR; Kùzu (CIDR 2023) for node groups and
columnar CSR; Salt (OSDI 2014), Callas (SOSP 2015) and Tebaldi (SIGMOD 2017) for concurrency control chosen per class of
transaction, the precedent of the storage lanes.
