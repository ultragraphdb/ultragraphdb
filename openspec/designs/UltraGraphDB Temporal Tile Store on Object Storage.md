# UltraGraphDB Temporal Tile Store on Object Storage

Nov 17, 2026 · @Marvin Hansen

This note records two investigations and combines them into one storage engine: whether a bi-temporal tile store
serves Scrith's knowledge propagation process and lifts writer coordination, and what S3 as a storage medium takes
once longshore issue #40 is implemented. It proposes; it changes no specification. Each open question is answered
from the specifications or marked as a decision.

**Decided before this note.** The write-ahead log stays local, synced per round, under both bindings (2026-11-17).

**Status of each part.**

| Part | Status |
| --- | --- |
| The bi-temporal model: `asserted`, revisions, retraction, `believed at` | Specified (`SCHEMA.md` section 8.4, `STORAGE.md` sections 7.1 and 14.3, `QUERY-ULTRA-QL.md` section 11) |
| S3 as the authority for closed extents | Specified as the `object` binding (`STORAGE.md` sections 12.5, 17.4, 17.7) |
| longshore as the object binding's client | Decided (`UltraGraphDB Storage IO in Longshore.md`) |
| Live and closed families of an edge-encoded `@bitemporal` relation; the start-epoch column of `@append_only` types; the interval tree | Decided 2026-11-17 and specified |
| Retraction as an append; compute reading published checkpoints; no failover by conditional writes | Decided 2026-11-17 and specified |
| Every measurement of the tile store | Not run: E1 to E14 of the tile-store note are planned |

## Findings

### Bi-temporal

1. **The model is specified, and Scrith never dropped it.** No note of Scrith's gives a missing clustered bi-temporal
   graph database as a reason for anything. On Dgraph the envelope was data every writer and every query maintained
   (`epi.recorded_at`, `epi.retracted_at`; `scrith/openspec/notes/schema/OLD/gen1` to `gen4`), because "no query reads
   the store as of an earlier one" (`scrith/bench/dgraph-issues.md` section 3). Gen5 declares
   `Statement @bitemporal(valid: valid)` (`scrith/openspec/notes/schema/gen5/DeepBrainSchema.ugs`), and `SCHEMA.md`
   section 8.4 is that model.

2. **Append-only lifts part of the coordination, and the design already takes that part.**

   | Lifted by immutability | Where |
   | --- | --- |
   | Recording a class I entry in a read set | `STORAGE.md` section 18.4 |
   | Read-set validation of a blind write | path A, tile-store note section 4.10; `SCHEMA.md` section 9.6 |
   | Folding and collecting class I entries | `STORAGE.md` section 14.3 |

   | Not lifted | Why |
   | --- | --- |
   | A total order on transaction time | `asserted` is epochs; the sequencer closes them. XTDB keeps one log and one writer, Datomic one transactor, TGMS a hybrid logical clock |
   | Two writers revising or retracting one instance | Both name the same statement; write-key admission orders them (path A keeps it) |
   | Constraints over the live slice | `@set`, cardinality, `@limit`, `@no_overlap`, `@acyclic` count live instances (`SCHEMA.md` section 8.4, rule 5) |
   | Key, `@unique` and `@idempotency` claims | Segmented (`SCHEMA.md` section 9.3) |
   | `@retracts` and `@ends` across partitions | Coordinated (`SCHEMA.md` section 9.3) |

   Graphiti shows the system without these: invalidation in the client, transaction time from the client's clock, no
   rollback on most of its backing stores (`graphiti_core/utils/maintenance/edge_operations.py`).

3. **Bi-temporality is a feature and not a throughput lever.** A relation's two delta entries against 9.1 store
   versions is the tile store's gain. A `@bitemporal` write costs more than a plain one: a version-run entry per
   revised attribute and a start-epoch column value.

4. **No published system pairs immutable bi-temporal records with CSR adjacency.** XTDB has no adjacency structure.
   AletheiaDB states "CSR" over a `DashMap` without describing it, and its own design note records unisolated
   checkpoints. TGMS names a "temporal CSR" its paper never describes and reports a two-hop snapshot at 13, 99 and
   272 ms p50 over 10^5, 10^6 and 10^7 edge versions (arXiv 2607.10265). LiveGraph and LLAMA are multi-version
   adjacency with transaction time alone.

5. **The knowledge propagation process maps onto the specified model:** `believed at` across retractions, both axes
   in one traversal, stale propagation through `grounds(with retracted){1,32}`, blast radius as a reverse read
   filtered by valid time, and what changed between two dates (`scrith/openspec/notes/schema/gen5/schema_graph_gen5.md`,
   R1, R2, R4). What Gen1 to Gen4 wrote in every writer becomes the engine's.

### Object storage

1. **The object binding is S3 as storage, with the log local.** The object store is the authority for a closed
   extent once published; a punched extent of the cold stream reads as zero, fails its tile's check and is repaired by
   a fetch; no commit waits on the store (`STORAGE.md` section 12.5).

2. **The binding writes each object once and needs four calls.** A PUT where no object exists, a GET, a listing by
   prefix and a delete. longshore implements each (`longshore_s3/src/s3/submit.rs`, `object_ops.rs`, `erase.rs`). It
   has no PUT under `If-Match`, and the binding needs none.

3. **Issue #40 is what fits the client to thread per core.** Today the client runs I/O threads beside its callers.
   With one `Io` per core serving files and sockets, a fetch, its write at the extent's address and the turn's reads
   share one submission, which stage 6.5's task 8.3 asks for.

4. **What a request costs, measured by longshore on AWS.** About 32 ms and 9 ms per MiB
   (`longshore/bench/results/m21-object-sizes.md`), so a cold extent of 1 MiB takes about 41 ms; a conditional PUT of
   4 KiB has a median of 131 ms (`m14-s3-aws-us-east-1.md`). A frontier request fetches one hop's extents together,
   so a cold traversal pays about one fetch per hop.

5. **What longshore lacks for the binding beyond issue #40.**

   | Gap | Consequence |
   | --- | --- |
   | Credentials are static keys or a reread file; no IMDS, IRSA or STS (`add-credential-providers`, 0 of 14 tasks) | `object_credentials_file` works; a pod's role does not |
   | MinIO and Ceph never run (path style is implemented) | The binding names both for on premises |
   | `aws-lc-sys` seals TLS records and needs a C toolchain | An accepted exception to the pure-Rust rule, as verified cryptographic code (decided 2026-11-17) |
   | The buffer pool is locked memory | A node's `RLIMIT_MEMLOCK` must be raised; the chart's pods take no `IPC_LOCK` |
   | Issue #40 keeps storage and network buffers apart | A fetched extent is copied once before its write, unless a body may be received into an aligned buffer (unverified) |
   | The simulator has no disk faults and no latency distribution | Issue #40 lists the first; `STORAGE.md` section 19.1 needs both |
   | A dropped `Pending` cancels nothing; a deadline does, checked every 50 ms | `object_fetch_ms` is served; a cancelled read's fetch runs on |

### Where the two meet

1. **History is cold by nature.** A class I tile is rewritten only when its row grows, so a tile of closed history
   ages past `cold_after_rounds` into stream 2 (`STORAGE.md` section 12.4), is published, and may be punched.

2. **Today history shares tiles with the live slice.** A retracted relation instance keeps its entries in its forward
   and reverse rows; a hub's row holds its whole history, which Scrith's Gen4 notes as "a reverse list grows with a
   thing's whole history". Live tiles are then rewritten with their history and never go cold.

3. **Liveness is a column apart from the entry.** `asserted_start_epoch` and `asserted_end_epoch` are column chunks
   by slot (`STORAGE.md` section 13.4), and an edge's inline columns are its ordering attribute and its declared
   fixed-width attributes, the two epochs not among them. A read of a `@bitemporal` relation's row therefore takes a
   column value per entry to tell live from retracted.

4. **Published checkpoints are a coarse second time axis.** A partition retains `object_checkpoints` checkpoint
   records, each a whole state at a log position, of every type.

## The engine

One engine, as stage 6.5 specifies it: a local log, a delta in memory, immutable checksummed tiles in a grid of
extents, compaction at round boundaries, replicas byte-identical. Three additions.

```
              round ──► local log (synced) ──► delta
                                                 │ compaction
                    ┌────────────────────────────┴───────────────────────────┐
              live families                                   closed families, @append_only    
              stream 1, hot                                   stream 2, cold
              local NVMe                                      published; punched; fetched on a read
                    │                                                        │
              reads at the present                            `believed at`, `with retracted`, `as of`
```

1. **Live and closed families of an edge-encoded `@bitemporal` relation.** Its forward and its reverse family exist
   twice: the live ones, class M, and the closed ones, class I. An insert writes the live families with the round's
   epoch as the entry's asserted start, an inline column. A retraction deletes the entry there and inserts it into the
   closed families with its start and its end, in one round; the round's derivations carry the entry, since apply
   reads no tile. A read at the present reads the live families and filters nothing; `believed at` reads both, each
   with an inline filter on the epochs that skips whole blocks; `with retracted` reads both unfiltered. A node-encoded
   relation and an index entry are not split.

2. **History reaches stream 2 by the rule that stands.** A tile goes cold when its rewrites are `cold_after_rounds`
   apart (`STORAGE.md` section 12.4), which a tile of closed history is and a tile still taking appends is not. A
   rule sending class I to stream 2 when written was considered and withdrawn: a run's or a row's last tile is
   rewritten with every append, and rewriting published extents leaves objects to delete. Under the object binding
   the object store then holds history and the node holds the live slice and a cache of `cache_extents`.

3. **longshore on the core's ring** as the object binding's client, and as the file path if E9 shows the gain stage
   6.3 sets.

What does not change: commits never wait on the object store. One replica restores to its latest published
checkpoint and loses the log above it (`STORAGE.md` section 17.7); stage 8 replicates the log.

## Questions

| # | Question | Answer |
| --- | --- | --- |
| Q1 | May a read go below `H` for class I? | **Decided 2026-11-17: no, and the promise holds by stored epochs.** The contradiction: `as of epoch e` is answered at every past point for `@bitemporal` and `@append_only` types (`QUERY-ULTRA-QL.md` section 11.1), while `STORAGE.md` refuses every read below `H` and folds created versions. An `@append_only` type now holds `ugdb:asserted_start_epoch`, a class I column, as a `@bitemporal` type does; compute reads at a version at or above `H` and keeps what stood at `e`. `H` stays the only bound on the versions a read may take. Not taken: created versions kept through the fold in every class I tile (the tile-store note's section 4.8). Specified in `SCHEMA.md` sections 7.4, 8.4 and 12.4, `CATALOG.md` sections 5.5 and 5.11, `STORAGE.md` sections 13.4, 18.6 and 20 |
| Q2 | How is a `@bitemporal` relation's row filtered by `asserted`? | **Decided 2026-11-17: it is not filtered at the present.** As first specified, by a column value per entry. Now the retraction moves the entry from the live families to the closed ones (addition 1) |
| Q3 | Should a retraction append and not write `asserted_end_epoch` in class M? | **Decided 2026-11-17: yes.** The end column is class I: a retraction writes it once into a slot that held none, which is an insert. A `@bitemporal` type is then wholly class I, and no tile of it carries a deleted version. Specified in `STORAGE.md` sections 13.4, 14.3 and 20, `SCHEMA.md` section 12.4 and `CATALOG.md` section 5.11 |
| Q4 | Do blind revisions take path A? | Yes, already. `@bitemporal` content is coordination-free (`SCHEMA.md` section 9.3), and a predicate with no check reading beyond the instance is blind (section 9.6). A retraction is Operation kind 6 with its `@on_retract` actions (`PROTOCOL.md` section 7.3) and takes a transaction or a ConditionalWrite |
| Q5 | How is `@index(interval)` stored? | **Decided 2026-11-17: as a relational interval tree's tokens** (`STORAGE.md` section 5.3.1). It was two range tokens on the bounds, so `valid at t` scanned every start at or below `t`. Each interval is now filed under its node of a virtual binary tree over the bound type, still two tokens, and a read takes at most two ranges per bit of a bound and one more, 273 for a temporal type, most of them empty seeks. A change of either bound rewrites both tokens, where it rewrote one. E18 measures it |
| Q6 | What does a version run of very many revisions cost? | Unknown. XTDB's resolution was quadratic for such entities until it changed its ceiling's order. To measure, E17 |
| Q7 | What bounds history under the object binding? | **A decision.** `grid_extents` × `extent_bytes` is 16 GiB by default, and a full grid is the persistent `GridFull` (`STORAGE.md` section 12.4). The distribution design leaves the grid's size under this binding open (U1). History kept for ever needs the bound set by the data and not by the drive |
| Q8 | Does compute read published checkpoints from the object store? | **Decided 2026-11-17: yes**, read-only, at a published checkpoint, after stage 8. Specified in `STORAGE.md` section 12.5, *Compute as a reader*. It serves projections for analytics and the audit replay of types that are not `@bitemporal` (Scrith's context projection), at checkpoint granularity, and adds no write path. It answers the distribution design's first open question |
| Q9 | Does a partition fail over by the object store's conditional writes? | **Decided 2026-11-17: no** (`STORAGE.md` sections 12.5 and 20). The log is local, so such a failover cannot recover the tail above the published checkpoint; stage 8's replication covers the case |
| Q10 | When is a legal purge complete under the object binding? | When the last retained checkpoint record naming an extent that held the instance expires (`STORAGE.md` section 12.5, *Retention*): after the rewrite, `object_checkpoints` further publications. Stated in `SCHEMA.md` section 8.8 |
| Q11 | Does storage run where `io_uring` is refused, and on macOS? | With longshore, only by issue #40's off-ring file path and readiness backend; today longshore's one backend on macOS is its simulator. Stage 6.3's question 7 |

## What each addition changes

| Addition | Changes |
| --- | --- |
| Live and closed families | Done: `STORAGE.md` sections 13.4, 14.3, 15.2, 18.5 and 20; `CATALOG.md` section 5.11; `PROTOCOL.md` sections 12.1 (derivation kind 4), 12.2, 13.13 and 13.21 (`slice`); `SCHEMA.md` section 12.4. No tile format changes: the two families differ in the class byte their key holds already, and the epochs are inline columns |
| Retraction as an append (Q3) | Done: `SCHEMA.md` section 12.4; `STORAGE.md` sections 13.4, 14.3 and 20; `CATALOG.md` section 5.11. The write-set record already holds a retraction as statements alone, so `PROTOCOL.md` does not change |
| The start-epoch column (Q1) | Done, as Q1 lists |
| The interval tree (Q5) | Done: `STORAGE.md` sections 5.3, 5.3.1 and 20; `SCHEMA.md` sections 5.5, 6 and 12.2; `QUERY-ULTRA-QL.md` section 11.3 |
| longshore as the client | Stage 6.5's object-store interface and its client group (design D3; task 0.7) |

Stage 6.5 has not built the grid, so none of this rewrites code. What stage 6.5 still owes these decisions is its
task 0.8: the conformance vectors, the Round's fuzz seeds and the Lean `RoundModel`.

**What the split costs.** 8 bytes an entry in the live families and 16 in the closed ones, fixed width; a retraction
writes four delta entries where it wrote none in these families; and a `@bitemporal` relation's live rows record in
lane X, as any class M row does.

## What to file at longshore

Beside issue #40: credential providers for a pod's role; a run against MinIO and Ceph; a body received into an aligned
registered buffer; a latency distribution in
the simulated store; cancellation of the operation behind a dropped `Pending`.

## Experiments

Beside E1 to E14 of the tile-store note. None has run.

| # | Measures | Compared with |
| --- | --- | --- |
| E15 | One hop at the present over a hub with 0%, 50% and 90% of its edges retracted, live rows against one row holding all of it | The same hub with no retraction |
| E16 | `believed at` over two hops at 10^5, 10^6 and 10^7 edge versions, history present and history punched | TGMS's 13, 99 and 272 ms |
| E17 | A version run of 10^6 revisions: the latest revision and one as of a past epoch | A run of 10 |
| E18 | `valid at t` over an interval index whose history is 10 and 100 times its live slice | The live slice alone |
| E19 | Scrith's InvalidationImpact over 5,460 nodes (`scrith/bench/dgraph-bench.md` section 8) | Dgraph 12.5 ms, SurrealDB 13.5 ms p50 |
| E20 | A cold extent's fetch and write at its address on one core's ring, at extents of 256 KiB, 1 MiB and 4 MiB | The distribution design's open question on the extent size |

## Sources

- `specification/storage/STORAGE.md`, `specification/schema/SCHEMA.md`, `specification/query/QUERY-ULTRA-QL.md`,
  `specification/binary_protocol/PROTOCOL.md`; `openspec/notes/UltraGraphDB Scalable Tile Store.md`;
  `openspec/designs/UltraGraphDB Distribution Architecture.md`, *Two bindings*;
  `openspec/designs/UltraGraphDB Storage IO in Longshore.md`; `openspec/benches/benches-2026-11-09.md`.
- Scrith: `openspec/notes/schema/` (gen1 to gen5, `OLD/ultragraphdb-gaps.md`), `docs/DeepBrain/`, `bench/`.
- longshore: its crates, `bench/results/`, `openspec/changes/`, and issue 40,
  https://github.com/carverauto/longshore/issues/40.
- XTDB: https://xtdb.com/blog/building-a-bitemp-index-2-resolution,
  https://xtdb.com/blog/building-a-bitemp-index-3-storage, https://xtdb.com/blog/taking-out-the-o-n-squared,
  https://docs.xtdb.com/ops/config/log/kafka.
- Graphiti: https://github.com/getzep/graphiti. AletheiaDB: https://github.com/autumn-foundation/AletheiaDB,
  `docs/ARCHITECTURE.md`, `docs/MVCC_SNAPSHOT_DESIGN.md`. TGMS: https://arxiv.org/abs/2607.10265.
- AeonG, https://arxiv.org/abs/2304.12212; LiveGraph, https://vldb.org/pvldb/vol13/p1020-zhu.pdf; LLAMA (ICDE 2015);
  Datomic's indexes, https://docs.datomic.com/indexes/indexes.html.
