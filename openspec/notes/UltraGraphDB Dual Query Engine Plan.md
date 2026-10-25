# UltraGraphDB Dual Query Engine Plan

Phase 1 of the dual-engine roadmap (`UltraGraphDB Roadmap Dual Query Engine and Tile Storage.md`) is feasible on the
existing storage engine. That roadmap calls its two parts "Stage 1" (conformance) and "Stage 2" (scale); this plan
calls them phase 1 and phase 2, to keep them apart from the nine stages of `ROADMAP.md`. SPARQL and GQL replace DQL;
fjall, the posting lists, MVCC at commit timestamps and the simulator stay. Three changes carry the plan: a second key
layout built for quads, a write path that executes transactions on n−1 cores and serialises only a short commit step,
and the removal of most reads from the write path. Together they address the measured bottleneck
(`perf-optimization.md`) and give both engines strictly serializable transactions.

The plan is large. The storage and concurrency work is bounded and builds on tested code; the two query engines, the
kernel, the protocol and the catalog are new code, and GQL has no official conformance suite. Those are the risks,
ranked at the end.

The plan covers stages 4 to 9 of `ROADMAP.md`'s nine ("Stages" below). Phase 2's tile storage lands with sharding and
clustering, in stage 9, so the distributed design is set out here as well.

## What stays, what changes, what goes

| Crate or change | Fate | Why |
|---|---|---|
| `ultragraphdb_store_kv`, `ultragraphdb_store_sim`, `ultragraphdb_traits_store`, `ultragraphdb_error_store` | Stay; gain a keyspace without versions | The store, its crash handling and the simulator are language-neutral |
| `ultragraphdb_store_posting` | Stays; its batch model changes | Lists, deltas, rollups, splits and the cache carry over; requests stop stacking on each other (see the write path) |
| `ultragraphdb_store_apply` | Stays as index maintenance and schema changes; loses its DQL edge input | Tokens, reverse edges, counts, rebuilds and drops carry over, driven by the catalog and a changeset format instead of `DirectedEdge` |
| `ultragraphdb_store_hnsw` | Stays, as an extension | Neither standard defines vector search; both engines reach it through an extension function |
| `ultragraphdb_core_codec`, `_algo` | Stay, as kernel operators | Sorted uid lists and their intersections are the kernel's intersection primitive |
| `ultragraphdb_core_types`, `_tok`, `_text` | Stay | Value conversion, tokenizers for indexes, collation |
| `ultragraphdb_core_keys` | Rewritten | The quad layout below replaces Dgraph's; byte compatibility with Dgraph ends, and its golden vectors retire |
| `ultragraphdb_core_schema` | Its state becomes the catalog; its DQL schema parser goes | SHACL and GQL graph types are the catalog's surface syntaxes |
| `ultragraphdb_core_dql`, `_lex`, `_chunker`, `_facets`, `ultragraphdb_client`, `ultragraphdb_proto_dgraph` | Removed | DQL, Dgraph's mutation formats and its gRPC API. RDF loading uses oxttl; facets become statement-ID properties |
| `port-stage-5-single-node` | Superseded by stages 5 and 8 | Its `query`, `worker/task.go` and `edgraph` ports go; its transaction work moves to stage 5, and access control and namespaces to stage 8 |
| `port-stage-6-replication` | Superseded by stage 9 | Its Raft port (`raftwal`, etcd raft) is not built: stage 9 replicates the ordered log with VSR. Its log storage, membership, snapshots and fault harness carry over |
| `port-stage-7-sharding-and-operations` | Superseded by stages 8 and 9 | Tablets, predicate moves and transactions across groups through Zero give way to tiles and the ordering service; export, backup, restore and the loaders move to stage 8, on quads |

The three changes are replaced when the specs of stages 5, 8 and 9 are written.

## The external pieces

| Piece | State (checked 2026-10-25) | Consequence |
|---|---|---|
| `spargebra` 0.4.8, `spareval` 0.2.9, `oxrdf`, `oxttl`, `sparesults` | MIT OR Apache-2.0, released October 2026, pure Rust (spareval's dependencies are hashing, regex, rand and Oxigraph's own crates) | The SPARQL parser and the reference evaluator are available; spareval's `QueryableDataset` trait (`src/dataset.rs:22`, `internal_quads_for_pattern`) is the seam for stage 6's reference oracle |
| selene-db (`selene-gql`) | Apache-2.0 only, 2.0.0-alpha.1, one contributor, no push since 2026-09-13; about 400 KB of parser, 340 KB of analyzer and a GQL Flagger | A usable base for the GQL front end, owned by this project once forked. The roadmap's "MIT/Apache-2.0" for it is wrong |
| W3C SPARQL 1.1 suite | 440 query and 148 update tests across 35 areas in `w3c/rdf-tests`, plus a SPARQL 1.2 tree | A complete specification oracle for the SPARQL side |
| openCypher TCK | 220 feature files, Gherkin | Runs on the repository's cucumber harness; no ISO GQL suite exists |
| MGQL (`EngineeringSoftware/mgql`, OOPSLA 2026) | MIT; Lean 4.14.0 with no dependency beyond core Lean; big-step and small-step semantics of a GQL fragment, proven equivalent; type soundness; a certified type checker; no `sorry`, standard axioms only; 350 executable assertions | A machine-checked reference semantics for the read core of GQL, used as below |

## GQL conformance with MGQL

GQL has no official conformance suite, and the openCypher TCK tracks Cypher. MGQL gives the GQL engine's read core a
reference semantics that is machine-checked rather than the project's own reading of the standard.

**The fragment it covers:**
- `USE`, `MATCH`, `WHERE` and `RETURN`;
- path patterns: all seven edge directions, label expressions, property constraints, quantified edges and paths, and
  pattern conjunction;
- the trail path mode;
- Kleene three-valued predicates with aggregates;
- projections with aliases and aggregation;
- `UNION`, `OTHERWISE`, `EXCEPT [ALL]` and `INTERSECT [ALL]`.

These are the places where GQL departs from SPARQL and Cypher.

**Outside it** (MGQL's `HOWTO.md`, "The supported fragment"):
- `WITH` pipelining, `OPTIONAL MATCH`, `ORDER BY` and `LIMIT`, and shortest paths;
- every write and every transaction;
- parsing: queries are Lean syntax trees, so syntax conformance stays with the parser and the Flagger.

It is the authors' formalisation of the standard: a disagreement between MGQL and the GQL engine is judged against the
ISO text, as every oracle difference is.

**Uses:**
1. **Golden results.** A generator produces property graphs and queries inside the fragment. An exporter writes each
   as an MGQL `PropertyGraph`, `GraphSchemaFull` and query term, MGQL evaluates them, and the results are committed as
   golden files. Lean runs outside Bazel, as Go does for `tools/golden`, so the GQL engine's tests stay hermetic and
   compare against committed files.
2. **Relations that cannot give false alarms.** MGQL's theorems hold under its semantics: 281 in `Metatheory.lean`,
   among them that trail-mode results are a subset of walk-mode results (`evalPatternTrail_subset`). Stated as property
   tests over the GQL engine, a failure is the engine's bug, which the metamorphic layer of the roadmap's testing needs.
3. **A typing reference.** The certified checker (`inferQuery`, with `inferQuery_sound` and `inferQuery_conforms`)
   infers a query's result schema from the graph schema; the GQL analyzer and the catalog's graph types are checked
   against it.
4. **Seed scenarios.** MGQL's 276 unit assertions, 30 worked examples and six LDBC SNB queries, each with its expected
   result, become Gherkin scenarios in the GQL scenario suite.

## Storage for two engines

The roadmap's storage changes (a graph component in the key, statement IDs, mandatory reverse indexes, type bits in
UIDs) are necessary and not sufficient. SPARQL also needs access paths that predicate-first keys do not offer, and RDF
values do not fit Dgraph's one-type-per-predicate schema.

**Terms are dictionary-encoded.** Every IRI, blank node and literal maps to a term ID; UIDs are the IDs of IRIs and
blank nodes, with the type bits the roadmap reserves. Literals keep their lexical form, datatype, language tag and
direction, so `"01"^^xsd:integer` and `"1"^^xsd:integer` stay distinct terms; how each kind is encoded is decided
under "Decisions" below. A predicate's objects are then terms of any kind, IRIs and literals mixed, which RDF data
requires and Dgraph's schema forbids.

**The dictionary is a keyspace without versions.** A term's ID never changes once assigned, so the dictionary needs no
MVCC: term → ID and ID → term are plain keys, fjall's bloom filters answer a lookup of an absent term with a point
get, and a read at an older timestamp seeing a newer term is harmless, since no quad visible at that timestamp names
it. Entries are written by the commit step only, so an aborted transaction leaves none.

**The quad layout.** Every keyspace below keeps versions in the key, as today:

| Keyspace | Key | Value | Serves |
|---|---|---|---|
| Forward | graph, predicate, subject | Posting list of object term IDs, with a statement ID where one is allocated | `s p ?o` |
| Reverse | graph, predicate, object | Posting list of subject IDs, for every object kind | `?s p o`, including bound literals |
| Subject predicates | graph, subject | The predicates the subject has | `s ?p ?o` |
| Object predicates | graph, object | The predicates pointing at the object | `?s ?p o` |
| Value indexes | graph, predicate, tokenizer, token | Subject IDs, as Dgraph's index keys | FILTERs and GQL property predicates the catalog indexes |
| Statements | statement ID | Its quad and its properties | RDF 1.2 reifiers, GQL edge identity and edge properties, parallel edges |
| Statistics | graph, predicate | Triple counts, distinct subjects and objects, degree histograms | The planners |

The subject-predicate and object-predicate lists make an unbound predicate a list read rather than a probe of every
predicate. Mandatory reverse lists over all objects make a bound literal object an index lookup without a declared
index.

**Absence proofs largely disappear.** Today's write path spends 34 % of the apply thread seeking keys that were never
written (`perf-optimization.md`). Under dictionary encoding, a term new to a transaction receives a fresh ID, and a
fresh ID has no stored versions under any key, so its lists are known empty without a read. The one remaining absence
proof per new term is a dictionary point get, which a bloom filter answers. The existence marker proposed in
`perf-optimization.md` is then needed only if a measurement still shows absence proofs on existing terms.

**Most writes need no read.** RDF predicates are sets: inserting a triple appends a posting, as do its reverse,
predicate-list and index entries; the port already writes such lists blind (`apply_request/mod.rs:196-207`). Reads
remain for three cases:
- a delete, which must keep the tokens the subject's remaining values still produce (ledger 35);
- a property the catalog declares single-valued, whose overwrite removes the old value's tokens: GQL's properties;
- an exact count; the planner's statistics are kept approximate, by deltas, and need none.

**Allocation.** The trims already made stay. Dictionary encoding replaces most stored keys with fixed-width IDs, which
removes most of the per-read key building measured in the profile.

## The write path on n−1 cores

The port and Dgraph both apply writes on one thread per group (`worker/draft.go:863`, `processApplyCh`), and the
superseded single-node change kept that thread as the serialisation point. The plan replaces it with FoundationDB's
ordering and acknowledgement, run inside one process, and validation in the conservative, deterministic form stage 9
runs across nodes. FoundationDB derives strict serializability from two rules: a transaction's read version is "no
less than any committed version when Tx starts", and its commit version "is larger than any existing read or commit
versions" (FoundationDB, SIGMOD 2021, §2.4.2).

1. **Execute anywhere.** A read-write transaction runs on any worker core against a snapshot at its read timestamp,
   the visibility watermark when it starts. It buffers its writes as a changeset of posting deltas, dictionary entries
   and statement records, and records its read set: the posting-list keys it read and the key ranges it scanned, so a
   phantom in a scanned range is caught.
2. **Order first, in epochs.** The commit step gathers the transactions that arrive within an epoch, which on one
   node is one group commit, and gives each its commit timestamp from the in-process oracle (an atomic counter: up to
   30 million timestamps a second, 8 million at 1,024 simulated cores; "Staring into the Abyss", VLDB 2014).
3. **Validate after, conservatively.** Each transaction is checked against the write sets of every transaction
   ordered after its read timestamp and before it, those that abort included, and aborts if any of them wrote what it
   read. Counting predecessors that abort can only add aborts, and it makes every check independent of every other, so
   validation runs on as many threads as an epoch needs and gives the same decisions every time; the form is the one
   stage 9 runs across nodes. Blind deltas are writes without reads, so two transactions appending to one hot list do
   not conflict. For scale, a single-threaded FoundationDB resolver handles about 280K transactions per second
   (SIGMOD 2021, §2.4.2).
4. **Write in groups, synced, in order.** Admitted changesets are written to fjall as one atomic batch per epoch, each
   at its own commit timestamp, as the posting crate's batches already are, and the batch is synced to disk before
   any of its transactions is acknowledged.
5. **Acknowledge at visibility.** The visibility watermark advances over the contiguous prefix of written timestamps;
   a transaction is acknowledged once the watermark reaches its commit timestamp, and every new transaction reads at
   the watermark or later. That is FoundationDB's argument for strictness, unchanged.

**Read-only transactions** read at the watermark on any core, are never validated and never abort.

**Interactive transactions** (GQL sessions, Bolt's explicit transactions) are the same transactions held open across
requests: optimistic execution needs no read or write set in advance. A read-write transaction lives at most a bounded
window, because its read timestamp holds the discard timestamp back; the limits are under "Decisions".

**Stage 9.** The single node's commit step becomes the ordering service, and the log it writes becomes the replicated
log ("The distributed design").

### What the alternatives give up

| Design | Why it is not the primary path |
|---|---|
| One serial apply thread (today, Dgraph, TigerBeetle) | Strictly serializable and simple, but writes stop at one core |
| Deterministic execution: Calvin, Bohm, PWV | Read and write sets must be known before execution; a graph traversal's depend on the data, and interactive transactions are impossible |
| Aria (deterministic batches, sets not needed in advance) | No interactive transactions, and a single transaction delayed 20 ms slows its batch by about 81 % (VLDB 2020, Fig. 5). It is kept as the fallback for bulk loads and contended one-shot streams |
| Silo, TicToc, Cicada | They scale by removing the single, central order: Silo orders within epochs it releases later, TicToc derives timestamps from the data, and Cicada keeps multiple versions under loosely synchronised per-thread clocks. The plan's strictness rests on one order and the visibility watermark, and TicToc and Cicada do not claim strictness |
| Shard-per-core write ownership (Seastar, Redpanda) | A graph write touches forward, reverse, predicate-list and index keys on different shards, so nearly every write becomes a cross-core two-phase commit |
| TigerGraph's model | Its current documentation claims "the read-committed isolation level" (TigerGraph 4.1, Transaction and ACID), its earlier claim of serializability withdrawn |

### What TigerGraph contributes

Its read side, not its transactions. Data lives in segments of a fixed number of vertices, the unit of parallel work;
each committed transaction has an ID, and a query at that ID reads a snapshot plus the deltas above it (TigerVector,
arXiv 2501.11216, §2, §4.2). The posting lists already are snapshot plus deltas at a timestamp. What carries over is
the unit of parallelism: a large query splits into morsels by subject ID range, the analogue of a segment, and its
operators combine partial results only through commutative merges at barriers, as TigerGraph's accumulators do
(Deutsch et al., arXiv 1901.08248, §5.2).

## Threads: dynamic thread-per-core

The host's cores are divided as follows, sized at start from the cores available to the process:

- **One core for the commit step**: oracle, validation and the group-commit writer. More validation threads come from
  the worker pool when an epoch needs them, since every check is independent.
- **n−1 worker cores**, each a compio runtime pinned to its core, running requests, read-write transactions and query
  morsels. fjall starts its own flush and compaction threads, which share these cores; fjall exposes no setting for
  their priority, so their cost is measured in stage 5.

The "dynamic" part is how work reaches the cores: a request enters the least-loaded core's queue, and a large query's
morsels go to a shared queue that idle cores take from. Pure thread-per-core without that sharing is unbalanced under
skew, because one supernode query occupies one core.

Thread-per-core alone does not make reads or writes vastly faster; it removes cross-core queueing and gives each core
its own allocator heap and caches, a gain to be measured, not assumed. The large gains come from three other places:
1. moving write execution from one core to n−1;
2. removing the absence proofs and the reads RDF semantics do not need;
3. morsel parallelism inside large queries.

**The ceiling.** After those changes, write throughput is bounded by the commit step and by fjall's ingest through its
one journal. The kv benchmarks (`cargo bench -p ultragraphdb_store_kv`) measure that ingest; the commit step's
per-transaction cost must stay well below the per-transaction execution cost divided by n−1, or the extra cores sit
idle.

**Where the design breaks, and the response:**
- **Hot read-then-write keys** (uniqueness checks, single-valued properties under contention) abort repeatedly under
  optimistic execution. The commit step routes a key whose abort rate passes a threshold to a pessimistic per-key
  queue, and contended one-shot streams to Aria-style batches.
- **Long read-write transactions** pay for large read sets and pin old versions. They are bounded by the window;
  analytics runs read-only.
- **Coarse read ranges** cause false aborts. Ranges are recorded at posting-list granularity, finer for index scans.
- **One large group commit** delays every later acknowledgement. Group commits are bounded in size.

## The distributed design (stage 9)

### What existing systems show

| System | Where an edge lives | Isolation | How a write to both sides commits |
|---|---|---|---|
| NebulaGraph | Twice, with the source and with the target, on hash partitions with Raft per partition | None across the two copies: TOSS makes them eventually consistent and is off by default "due to performance and stability considerations" | Not atomic |
| ByteGraph (PVLDB 15) | Twice: adjacency keyed ⟨vID, vType, eType, dir⟩ | Read committed: "Currently we do not consider higher isolation levels since RC can satisfy most requirements of our applications" | Two-phase commit with write intents, no MVCC |
| Weaver (PVLDB 9) | Once, with the source: a partition holds "all outgoing edges rooted at those vertices" | Strictly serializable | Ordering is its own layer: gatekeepers exchange vector clocks every τ ms, a timeline oracle orders only concurrent transactions that conflict, writes commit through HyperDex Warp's optimistic protocol, and shards apply "without coordination" |
| A1 (SIGMOD 2020) | Edge data once, with an edge list of pointers at each endpoint, as ByteGraph's paper describes it | Serializable with opacity, on FaRM, as ByteGraph's paper describes it | A two-phase commit variant over RDMA |
| G-Tran (PVLDB 15) | Edge object once, with an edge cell at each endpoint pointing to it | Serializable and snapshot isolation | Multi-version optimistic concurrency, decentralised, over RDMA |

Two things follow:

- **Storing both directions is not the cost.** The cost is the two copies sitting in different commit domains whose
  storage shards must agree on each commit.
- **Strict serializability's cost is in the read set.** G-Tran names it: "Read-only graph transactions tend to have
  large read sets since a large number of vertices and edges can be easily involved after just two to three hops",
  which with long transactions brings "high abort rate and low throughput". Weaver's throughput "decreases as the
  percentage of writes increases", because "the timeline oracle serializes concurrent transactions that modify the
  same vertex". Its implementation orders any concurrent transactions sharing a shard server, not only those sharing a
  key. Even so, only 0.0013 % of the TAO workload's transactions and 1.7 % of a 75 %-read mix needed ordering.

### The design

Each part removes one of the costs above:

1. **Read summaries.** A read-write transaction ships a summary of what it read, not the read set:
   - exact keys and ranges while they are few;
   - past a budget, its point reads as a Bloom filter of their keys, one hash insert per read during execution, with
     the coarse regions (tiles, or key ranges) they fall in;
   - every range it scanned, kept as a range at every size, coarsened to a covering range past the budget. A scan
     depends on keys it never read: a key inserted into the range later is a phantom no filter of read keys can hold.
2. **Writes are checked against reads.** Validation walks the write sets committed between the transaction's read and
   commit timestamps:
   - a write inside a scanned range conflicts;
   - a write inside a point-read region is probed against the filter. A Bloom filter never misses a key it holds, so a
     real conflict on a point read is caught, and a false positive only adds an abort.

   Validation cost follows the writes in a transaction's regions during its lifetime, which the 10 s limit bounds, and
   not the data's size. The filter does not escape the read set's size:
   - its size grows with the keys read, and it travels to the ordering service;
   - its false positives compound with the writes probed against it: with W writes probed and a per-probe false
     positive p, a false abort has probability about 1 − (1 − p)^W.

   | Bits per key | p | False abort at 1,000 writes probed | Filter at 1M keys read |
   |---|---|---|---|
   | 10 | 0.8 % | about 99.97 % | 1.25 MB |
   | 20 | 0.007 % | 6.5 % | 2.5 MB |
   | 30 | 0.00006 % | 0.055 % | 3.75 MB |

   The summary pays where point reads are many and the writes in their regions few; the budget and the bits per key
   are set from stage 5's measurements. Signature-based conflict detection of this kind is known from transactional
   memory (Bulk, ISCA 2006, cited from memory).
3. **Order first, in epochs; validate after, deterministically.**
   - **Ordering:** a sequencer orders transactions in epochs of a few milliseconds, as Calvin does, so its cost is per
     epoch.
   - **Validation:** each transaction is checked against the proposed write sets of all its predecessors, including
     those that abort. That is conservative, and it makes every check independent of every other: validation runs in
     parallel and gives the same decisions on every replica. A hash of the decisions per epoch detects a replica that
     diverges.
4. **Edges are placed once, by tiles.** An edge is written into the tile of its source's row block and target's column
   block, which serves both directions. A high-degree vertex's row spreads across column tiles, and inserts into its
   lists are blind deltas that do not conflict.
5. **The log is the only state that must not be lost.** It is replicated by quorum before a transaction is
   acknowledged; tiles are derived state, rebuilt from a checkpoint and the log, and applied deterministically.

**Storage and compute separate along these lines:**

| Role | Does | State |
|---|---|---|
| Compute nodes | Run the engines; execute transactions against snapshots; build changesets and read summaries | None that must survive |
| The ordering service | Sequencer, epochs, deterministic validation, the ordered log | The log, replicated with VSR |
| Storage nodes | Tile groups that apply the decided log and serve snapshots at a timestamp | Tiles and their KV delta store, rebuildable from the log |

Dgraph's Zero and Alpha split is not built: Zero's timestamps and decisions become the ordering service, and the
Alpha divides into compute and storage. Stage 5 builds the same three roles inside one process, with interfaces shaped
as messages, so stage 9 changes the transport between them and not their semantics.

**What it costs:**
- **False aborts:** from coarse regions, filter false positives and counting predecessors that abort. They are tuned by
  filter size and region granularity, with exact read sets below the budget.
- **Latency:** a strict acknowledgement waits for its epoch to close, a few milliseconds, as in Calvin.
- **Inherent aborts:** a read-write transaction over a hot, write-heavy region still aborts under serializability;
  read-only work does not.
- **Fan-out:** the log is partitioned by region with a global epoch merge.
- **Compaction:** hot-tile compaction under heavy writes is the dual roadmap's open phase-2 risk.

## Stages

| Stage | Contents | Exit |
|---|---|---|
| **1. Foundations** | Done | Archived 2026-10-23 |
| **2. Storage** | Done, the fault simulator (1.5) included | Archived 2026-10-25 |
| **3. Website and public repository** | Done | — |
| **4. Data model and quad storage** | Mapping contract v1; catalog v1; the term dictionary, a keyspace without versions, in the `Store` trait, fjall and the simulator; the quad keyspaces; statement IDs; type bits in UIDs; changesets in place of `DirectedEdge`; index maintenance driven by the catalog; the DQL crates and Dgraph's gRPC API removed | Quads round-trip with statement IDs under the simulator's faults; `store_bench` loads quads and reports reads per relation, which fall to the dictionary lookups |
| **5. Transactions and the binary protocol** | The commit step in its distributed shape: order, deterministic validation, apply; read summaries; epochs as group commits; acknowledgement at the watermark; the ordered-log interface with a local implementation; compute, ordering and storage as separate components in one process; n−1 compio workers with dynamic dispatch; the per-key path and the Aria fallback. The binary protocol in a crate of its own: envelope, columnar results, handshake, multiplexing, cancellation, flow control, prepared statements, its conformance suite of valid and malformed frames, and a Rust client; the single-node server process | Elle checks strict serializability; throughput at 1, 2, 4, 8 and n−1 workers, with abort rates; read summaries' false-abort rates measured against write logs on the simulator; the protocol conformance suite passes |
| **6. SPARQL engine** | The kernel (access patterns, operators, statistics); spareval on the quad keyspaces through `QueryableDataset`; the W3C suites in CI; the native engine with binary and worst-case-optimal joins and morsel parallelism; the vector index as an extension function | The W3C SPARQL 1.1 query and update suites pass natively; no unexplained difference from spareval; SPARQL baselines |
| **7. GQL engine and Bolt** | selene-gql forked; the Flagger and feature register; the GQL planner on the kernel; MGQL's exporter, golden results, theorem-based property tests and seed scenarios; GQL writes through the changeset path; the Bolt adapter, the limit's abort classified as transient; the vector index as an extension function | The GQL scenarios and the openCypher TCK at an agreed rate; no unexplained difference from MGQL inside its fragment; Neo4j drivers in two languages run the TCK through Bolt |
| **8. Single-node product** | Access control and namespaces on the catalog; backup, restore and export; the live and bulk loaders, on quads, resolving every IRI in a dictionary pass first; administration and metrics; client libraries and their documentation, the transaction limits included; the optional SPARQL Protocol gateway | The dual roadmap's phase-1 exit gate: published conformance numbers for SPARQL 1.1, the openCypher TCK and the GQL feature register; LDBC and SPARQL baselines; the mapping contract at a stable version |
| **9. Distributed tiles** | First on one node, unreleased: read-only tiles built from the KV store, then tiles as a read path, with storage-differential testing. Then the ordering service over a VSR-replicated log; compute and storage nodes; tile placement, moves and rebalancing; membership; tile paging for graphs larger than memory; the multi-node deterministic simulator; Jepsen and Elle under faults | Every conformance suite gives identical results on the KV and the tile paths; Elle checks strict serializability under faults; a group failure loses no data; a graph larger than one node's memory runs the benchmark suites |

After stage 9: UltraQL, the dual roadmap's M9 and M10.

### Decided ahead, planned per stage

What crosses a stage boundary as an interface is decided ahead, because it is cheap to fix now and costly to
retrofit. Everything behind those interfaces is planned in its own stage.

| Item | Decided now | Planned in its stage |
|---|---|---|
| Storage and compute separation | The three roles above, as the target; stage 5 builds them in one process; Zero and Alpha are not built | Stage 9: deployment, placement, failure handling |
| VSR in place of Raft | The ordered-log interface (append a decided epoch, read in order, durable before acknowledgement), defined in stage 5 with a local implementation; Raft is not built | Stage 9: the VSR protocol, view changes, recovery |
| The binary protocol | Nothing in the engines or the server depends on gRPC or protobuf types; stage 4 removes Dgraph's API | Stage 5: the byte layout, the conformance suite, the client |
| Changesets, read summaries, region identity, UID type bits | Their shapes, so stage 9's tiles and log carry them unchanged | — |
| Tile format, rebalancing policy, the Jepsen suite | — | Stage 9 |

## Risks, by impact

1. **The GQL engine.** No ISO suite exists, the TCK tracks openCypher rather than GQL, and the fork base is a
   single-maintainer alpha. MGQL makes the read core's semantics machine-checked; outside its fragment (pipelining,
   `OPTIONAL MATCH`, ordering, shortest paths, writes, parsing), conformance claims rest on the project's own scenarios
   and the feature register.
2. **The mapping contract.** Edge identity, multi-valued properties and datatypes decide whether one store serves both
   views; a gap found after stage 4 changes the layout.
3. **Contention under optimistic execution.** Counting predecessors that abort raises the abort rate where transactions
   conflict. The per-key path and the Aria fallback are the answers; their thresholds are unmeasured.
4. **The commit step as a new ceiling.** If its cost per transaction approaches the execution cost divided by n−1,
   cores beyond that add nothing; stage 5 measures it before stage 6 depends on it.
5. **Scope.** Two engines, a kernel, a protocol and a catalog are new code of a size well beyond stages 1 and 2
   together; the storage and concurrency work is the smaller part.
6. **Stage 9 lands three things at once:** a new storage engine, sharding and replication. The unreleased single-node
   tile milestone, storage-differential testing and the multi-node deterministic simulator take it in steps.
7. **Read summaries' false aborts.** If coarse regions or filter false positives abort too many read-write
   transactions, validation falls back towards exact read sets and their cost; stage 5 measures the rate before stage
   9 depends on it.

## Decisions

### How long a transaction may stay open

| System | Documented limit | Isolation it pairs with |
|---|---|---|
| Neo4j | `db.transaction.timeout`, default `0s`: no limit | Read committed |
| TigerGraph | A query is the transaction; the 3.x default query timeout is 16 s, raised per request with a `GSQL-TIMEOUT` header | Read committed |
| NebulaGraph | No multi-statement transactions; TOSS keeps an edge's two records eventually consistent | None |
| JanusGraph | No transaction timeout; locking only where a schema element enables it | That of its backend |
| FoundationDB | 5 s, hard (the MVCC window) | Strict serializability |
| Spanner | A read-write transaction idle for 10 s may be aborted | Strict serializability |
| TiDB | `max-txn-ttl`, default 60 min | Snapshot isolation, optimistic or pessimistic |

The graph databases set no limit, or a long one. None of them validates reads, and a read-committed transaction can
run as long as it likes at little cost; that this is their reason is an inference, since none of them states one.
Under optimistic, strictly serializable execution a long read-write transaction pins old versions and grows more
likely to abort, which is why FoundationDB and Spanner keep their limits short.

- **Read-write transactions** live at most **10 s** by default, between FoundationDB's 5 s and TigerGraph's 16 s.
  Spanner's 10 s applies to idle time; here it bounds the whole lifetime, which is stricter. The limit is a setting,
  as Neo4j's `db.transaction.timeout` is, and a transaction past it is aborted at its next request or at commit.
- **Read-only transactions and queries** are bounded by the query timeout, **16 s** by default as TigerGraph's, raised
  per request. They are never validated, so a longer limit costs only version retention: the discard timestamp follows
  the oldest open snapshot.

The stance behind the limits: UltraGraphDB bounds a transaction's lifetime so that strict serializability stays cheap,
where read-committed graph databases leave transactions unbounded and weaken isolation instead.

**Bolt clients.** A read-write transaction past its limit is aborted with an error the Bolt adapter classifies as
transient, so a Neo4j driver retries it. A driver's managed transaction functions (`executeWrite`, `executeRead`)
retry transient errors with exponential backoff from 1 s within a 30 s budget by default (`maxTransactionRetryTime`,
Neo4j .NET and JavaScript driver documentation). A retry starts a new transaction at a new read timestamp. An explicit
transaction (`beginTransaction`) is not retried by the driver. The client documentation states the 10 s limit and its
setting, recommends managed transaction functions with idempotent units of work, and asks that waiting on a user or an
external call happen outside a transaction.

### What an acknowledgement promises

An acknowledged commit survives power loss. On one node, each epoch's batch is synced to disk before any of its
transactions is acknowledged; the batch shares one sync among its transactions, and stage 5 measures the sync's cost.
From stage 9, the replicated log takes this role: an epoch is acknowledged once a quorum of the ordering service holds
it durably.

### What a conflict is checked on

The read set records what a transaction observed, at the narrowest key it read:

| What the transaction did | Recorded as | Conflicts with a transaction ordered after its read timestamp that |
|---|---|---|
| Read a list: the objects of a subject and predicate | The list key (graph, predicate, subject) | Writes to that list |
| Tested one statement: existence, or the statement a delete removes | The statement (graph, predicate, subject, object) | Adds or removes that object in that list |
| Scanned: an unbound subject, an index token range, an edge iteration | The key range scanned | Writes a key inside the range |
| Wrote blind: inserts, reverse, predicate-list and index deltas | The write set only | Nothing; blind writes never conflict with each other |

- **Strictness is kept** because validation covers everything recorded, and coarsening only adds conflicts. A read
  set past its budget is coarsened to the ranges covering it, and predecessors that abort are counted; both can only
  add aborts.
- **Performance is kept** because hot lists are mostly written blind: an `rdf:type` reverse list or a popular index
  token takes concurrent inserts without aborting anyone. A transaction that tested one statement in a hot list
  conflicts only with writes of that statement. A transaction that iterated the whole list conflicts with every insert
  into it; that is the phantom serializability forbids, and a read-only transaction avoids it by reading a snapshot.
- **The write record carries objects:** a commit records its list keys and, for lists it changed by a few objects,
  the objects. Past a bound it records the list key alone.
- **Hot read-then-write keys** (a single-valued property under contention, a uniqueness check) move to a pessimistic
  per-key queue in the commit step once their abort rate passes a threshold. The queue serialises them, so strictness
  holds there too.
- **The record of recent write sets** holds every ordered transaction's writes, committed or aborted, by timestamp.
  It is pruned at the oldest open read timestamp, which the 10 s limit bounds, and held as a version-augmented range
  structure, as FoundationDB's resolvers hold theirs.

### How terms are encoded

Oxigraph 0.5.13 (`src/storage/numeric_encoder.rs`) stores a short string inline in 16 bytes, and longer strings and
every IRI as a 128-bit SipHash-2-4 of the string. Its dictionary maps the hash to the string and is filled by
insert-if-absent, so an encoded term is at most 33 bytes and needs no coordination. It also stores typed literals by
value (`parse_integer_str` parses, so `"01"^^xsd:integer` becomes `1`), merging terms that RDF keeps distinct. The
port keeps the inline and hashed forms and not the canonicalisation.

- **IRIs and blank nodes** get allocated 64-bit UIDs, carrying the type bits, through the dictionary. The posting
  lists, the uid codec and stage 9's tiles work on 64-bit UIDs, and a 64-bit hash would not do: by
  the birthday bound, a billion IRIs collide with probability about 2.7 %. A transaction creating a new IRI records its
  dictionary lookup in its read set; two transactions creating the same IRI at once conflict, and the second retries
  and finds the UID. Parallel loads, whose chunks share new IRIs, would conflict often, so the live and bulk loaders
  resolve every IRI in a dictionary pass before they write quads, as Dgraph's bulk loader assigns UIDs first; the
  commit step stays short.
- **Literals** are encoded by content, as one tag byte and up to 32 bytes, with no allocation and no read:
  - Booleans, integers within 64 bits, decimals, doubles, floats, and the date, time and duration types are stored by
    value **only when their lexical form is canonical**, that is, when it round-trips through the value. Otherwise
    they are stored as a typed literal with their lexical form. RDF term identity stays exact, and the common
    canonical case keeps native comparison and ordering.
  - Strings, language-tagged strings and other typed literals keep a lexical form, and a language tag, of at most 15
    bytes inline; a longer one is a 128-bit hash naming its dictionary entry.
  - The hash is BLAKE2b truncated to 128 bits; `blake2` is already a workspace dependency, for the hash tokenizer.
    SipHash with a public, fixed key is not designed to resist chosen collisions, and the input is untrusted (an
    inference, not a measured weakness).
- **Literal dictionary entries are written blind:** equal strings produce equal keys and equal values, so concurrent
  inserts commute and are left out of conflict checks.
- **Dictionary entries are not removed before stage 9.** A removal outside the commit step races a commit that writes
  the same entry again: if the removal lands after it, the new quad names a hash with no entry. A removal is therefore
  a commit of its own, ordered by the commit step and refused if any commit after its scan wrote the entry or a quad
  naming it.
- **In a posting list** a literal object is a value posting whose value is its encoded term. The posting's 64-bit
  fingerprint orders the list, and ties compare the encoded terms, so a fingerprint collision never merges two values.

### The vector index

It stays, as an extension: SPARQL reaches it through an extension function, and GQL through a procedure
call or extension function, in stages 6 and 7.

## Open questions

- Which datatypes beyond XSD get an encoding by value, for example GeoSPARQL's `geo:wktLiteral`?
- Do workloads at this scale stay read-dominated, as ByteGraph's 90–99.9 % reads and Facebook's TAO are? A
  write-dominated one moves the cost back into validation.
- Are read-write transactions with large traversals rare enough, with analytics running read-only?
- Is a few milliseconds of epoch latency on every acknowledgement acceptable?
