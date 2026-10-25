# UltraGraphDB Dual Query Engine Plan

Phase 1 of the dual-engine roadmap (`UltraGraphDB Roadmap Dual Query Engine and Tile Storage.md`) is feasible on the
existing storage engine. That roadmap calls its two parts "Stage 1" (conformance) and "Stage 2" (scale); this plan
calls them phase 1 and phase 2, to keep them apart from the stages of `ROADMAP.md`. SPARQL and GQL replace DQL;
fjall, the posting lists, MVCC at commit timestamps and the simulator stay. Three changes carry the plan: a second key
layout built for quads, a write path that executes transactions on n−1 cores and serialises only a short commit step,
and the removal of most reads from the write path. Together they address the measured bottleneck
(`perf-optimization.md`) and give both engines strictly serializable transactions.

The plan is large. The storage and concurrency work is bounded and builds on tested code; the two query engines, the
kernel, the protocol and the catalog are new code, and GQL has no official conformance suite. Those are the risks,
ranked at the end.

The plan covers stages 4 to 10 of `ROADMAP.md` ("Stages" below). The DeepBrain MVP runs on the single node of stage 7,
so stage 6 builds the query core both languages share, and full conformance and Bolt follow the single node. 

Phase 2's tile storage lands with sharding and clustering, in stage 10, so the distributed design is set out here as well.

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
| `port-stage-5-single-node` | Superseded by stages 5 and 7 | Its `query`, `worker/task.go` and `edgraph` ports go; its transaction work moves to stage 5, and access control and namespaces to stage 7 |
| `port-stage-6-replication` | Superseded by stage 10 | Its Raft port (`raftwal`, etcd raft) is not built: stage 10 replicates the ordered log with VSR. Its log storage, membership, snapshots and fault harness carry over |
| `port-stage-7-sharding-and-operations` | Superseded by stages 7 and 10 | Tablets, predicate moves and transactions across groups through Zero give way to tiles and the ordering service; export, backup, restore and the loaders move to stage 7, on quads |

The three changes are replaced when the specs of stages 5, 7 and 10 are written.

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

Stage 6 uses the golden results for queries inside both the fragment and the query core. Stage 8 uses all four, over
the whole fragment.

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
remain for four cases:
- a delete, which must keep the tokens the subject's remaining values still produce (ledger 35);
- a property the catalog declares single-valued, whose overwrite removes the old value's tokens: GQL's properties;
- a predicate the catalog declares unique, whose check reads the predicate's reverse list for the object;
- an exact count; the planner's statistics are kept approximate, by deltas, and need none.

**Allocation.** The trims already made stay. Dictionary encoding replaces most stored keys with fixed-width IDs, which
removes most of the per-read key building measured in the profile.

## The write path on n−1 cores

The port and Dgraph both apply writes on one thread per group (`worker/draft.go:863`, `processApplyCh`), and the
superseded single-node change kept that thread as the serialisation point. The plan replaces it with FoundationDB's
ordering and acknowledgement, run inside one process, and validation in the conservative, deterministic form stage 10
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
   stage 10 runs across nodes. Blind deltas are writes without reads, so two transactions appending to one hot list do
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

**Stage 10.** The single node's commit step becomes the ordering service, and the log it writes becomes the replicated
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

## The distributed design (stage 10)

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
as messages, so stage 10 changes the transport between them and not their semantics.

**What it costs:**
- **False aborts:** from coarse regions, filter false positives and counting predecessors that abort. They are tuned by
  filter size and region granularity, with exact read sets below the budget.
- **Latency:** a strict acknowledgement waits for its epoch to close, a few milliseconds, as in Calvin.
- **Inherent aborts:** a read-write transaction over a hot, write-heavy region still aborts under serializability;
  read-only work does not.
- **Fan-out:** the log is partitioned by region with a global epoch merge.
- **Compaction:** hot-tile compaction under heavy writes is the dual roadmap's open phase-2 risk.

## The query core (stage 6)

The DeepBrain MVP needs CRUD and common queries, not either language in full. Stage 6 therefore builds what SPARQL and
GQL share and both state plainly. Everything else waits for stage 8: the parts where the two diverge, the corner
cases, and full conformance testing. The core is ISO GQL. Cypher's `MERGE` and `WITH` are not in it ("Decisions").
DeepBrain writes each requirement in the language that states it best, so both engines earn their place from the MVP
on ("Which language DeepBrain writes in").

| Area | SPARQL 1.1 | ISO GQL |
|---|---|---|
| Patterns | Basic graph patterns over one graph or a named graph | Fixed-length node and edge patterns in either direction, label tests |
| Alternatives of relations | `p1\|p2` paths | Label expressions `A\|B` |
| Bounded walks over one relation or an alternative | `p+`, `p*`, `^p` | Quantified edges `{m,n}` |
| Filters | `=`, `!=`, `<`, `<=`, `>`, `>=`, `&&`, `\|\|`, `!` over typed literals | The same comparisons and `AND`, `OR`, `NOT` |
| Absence of a property | `FILTER NOT EXISTS` over one triple pattern of a bound subject | `IS NULL` |
| Projection | `SELECT` with aliases, `DISTINCT` | `RETURN` with aliases, `DISTINCT` |
| Order and paging | `ORDER BY`, `LIMIT`, `OFFSET` | `ORDER BY`, `LIMIT`, `OFFSET` |
| Aggregates | `COUNT`, `SUM`, `MIN`, `MAX`, `AVG`, `GROUP BY` | The same, with grouping keys |
| Composition | Sub-`SELECT` | `NEXT` |
| Writes | `INSERT DATA`, `DELETE DATA`, `DELETE`/`INSERT` … `WHERE` | `INSERT`, `SET`, `REMOVE`, `DELETE`, `DETACH DELETE` |
| Parameters | Values bound through the binary protocol's prepared statements | `$name` |
| Vector and text search | Extension functions | `CALL` … `YIELD` |

- **Walks answer the nodes they reach.** In the core, a walk's result is the set of nodes reached. A GQL quantified
  edge must be projected with `DISTINCT` or aggregated over distinct nodes. Path variables, path modes and per-path
  multiplicities are stage 8's: there GQL's walk semantics and SPARQL's set semantics part.
- **Outside the core means refused.** A query using a feature outside the core is refused with the feature's name.
  The core's register is the first part of stage 8's feature register.
- **Text search** reaches the term and full-text tokenizers of stage 2 through the same extension seam as the vector
  index, since neither language defines it.
- **Catalog declarations** arrive through the binary protocol: predicates, kinds, cardinality, datatypes, indexes and
  unique values. They are written as stage 4's changeset entries. SHACL and GQL graph types are stage 8's surface
  syntaxes for the same declarations.
- **Tests:**
  - the W3C SPARQL 1.1 tests whose features lie inside the core;
  - MGQL's golden results for queries inside both its fragment and the core;
  - differential tests that ask one question in SPARQL and in GQL over the same data, and compare the answers;
  - the DeepBrain MVP's queries and scrith's catalogue reads as scenarios.

## What scrith's ontology needs from the single node

scrith's ontology (`scrith/openspec/notes/schema/gen4/schema_graph_gen4.md`, with `extract-deep-brain-core` D43–D47)
was designed against Dgraph. It reaches a store through deep_brain's `Ontology` and `Projection` traits (D37, D38). An
UltraGraphDB backend implements those traits over the Rust client; that backend is scrith's work, after stage 7.
Every operation below is served by stages 4 to 7:

| The ontology needs | On Dgraph | On the single node | Stage |
|---|---|---|---|
| One node per identity | `epi.id @upsert` | A unique-value declaration on `epi.id` in the catalog, refusing a second subject with the same value | 4, 5 |
| Checks that decide a refusal: `Hierarchy`, `LiveUnit`, one `Preferred` per subject | `epi.guard` and an `@upsert` key per check, because snapshot isolation validates no read and an index read missed commits (D4-G, D44) | Validation covers every read, index ranges and reverse lists included, so the check's read is enough; the guards still work if written | 5 |
| Typed relations: `rel.<Kind>`, `stmt.<Kind>`, `MemberOf` typed by the member's kind | Per-kind predicates | Predicates the catalog declares, one per kind, as quads | 4 |
| A supertype as the union of its subtypes' edges (A3) | Several edges in one block | Alternatives of relations | 6 |
| `InUnit`, `KindInUnit`, `UnitsOf`; `Broader`, `Within` | `@recurse` to a depth | Bounded walks; D46 bounds the tree's depth | 6 |
| Open and live: `NOT has(time.valid_until)`, `NOT has(epi.retracted_at)` | `has()` | Absence of a property | 6 |
| History over a period: `time.valid_from` between two dates | `between` on a day index | A range filter on a datetime index | 4, 6 |
| `time.periods` and `space.cells`, many ints per node, in a range | `between` on an `[int]` index | A range over a multi-valued predicate, in SPARQL: each value is a triple of its own, so the range matches when any value lies in it | 6 |
| `concept.embedding`, cosine | `hnsw` | The vector index | 6 |
| `concept.definition`, labels | `fulltext`, `term` | Text search | 6 |
| `space.geo`, `within` and `near` | `geo` | Not in the core: S2 cell ranges over `space.cells` answer an area. Geo functions and their datatypes come with GeoSPARQL | 8 |
| The allocator's counters | One counter node per number, each in its own transaction | The same; a contended counter takes the per-key path | 5 |
| A read and the write it decides in one request | Upsert blocks | One read-write transaction | 5 |
| Keyset paging, `@cascade`, counts per kind | `first`, `after`, `@cascade`, `count` | Ordering and paging, required patterns, aggregates | 6 |
| Moves of 1,000 members per transaction (D46) | One transaction per batch | The same, inside the 10 s limit | 5 |
| The feed: an outbox record per write, numbered by a cut (D42, D45) | `has()` over pending flags rotated by epoch, created and dropped by schema | The commit log: each write attaches its record, and the commit timestamp orders it; no cut, sequencer or epoch flags ("The commit log") | 7 |
| The namespace pause for index changes and non-additive upgrades | A directory marker, the write deadline W, a drain | Runs unchanged; it needs transactions alone | 5 |
| A namespace per tenant; the PERSONAL predicates confined to the gateway | Namespaces; predicate ACL | Namespaces and access control on the catalog | 7 |

Two Dgraph findings become tests in stage 5.5: Q1, where an indexed root missed commits below its read timestamp, and
A8, where a hub's reverse edge missed them. On the single node a snapshot at R returns every commit below R, and the
tests hold it to that.

## Stages

| Stage | Contents | Exit |
|---|---|---|
| **1. Foundations** | Done | Archived 2026-10-23 |
| **2. Storage** | Done, the fault simulator (1.5) included | Archived 2026-10-25 |
| **3. Website and public repository** | Done | — |
| **4. Data model and quad storage** | Mapping contract v1; catalog v1; the term dictionary, a keyspace without versions, in the `Store` trait, fjall and the simulator; the quad keyspaces; statement IDs; type bits in UIDs; changesets in place of `DirectedEdge`; index maintenance driven by the catalog; the data dictionary's read API, per-label shapes from a background pass; the DQL crates and Dgraph's gRPC API removed | Quads round-trip with statement IDs under the simulator's faults; `store_bench` loads quads and reports reads per relation, which fall to the dictionary lookups |
| **5. Transactions and the binary protocol** | The commit step in its distributed shape: order, deterministic validation, apply; read summaries; epochs as group commits, each closing when the previous sync completes; acknowledgement at the watermark; the ordered-log interface with a local implementation; compute, ordering and storage as separate components in one process; n−1 compio workers with dynamic dispatch; the per-key path and the Aria fallback. The binary protocol in a crate of its own: envelope, columnar results, handshake, multiplexing, cancellation, flow control, prepared statements, its conformance suite of valid and malformed frames, the describe request over the data dictionary, and a Rust client; the single-node server process | Elle checks strict serializability; throughput at 1, 2, 4, 8 and n−1 workers, with abort rates; read summaries' false-abort rates measured against write logs on the simulator; match-then-insert over a Zipf-distributed set of entities at 8 writers, with and without the per-key path; D46's batch of 1,000 member moves; the protocol conformance suite passes |
| **5.5. Hardening transactions with chaos testing** | Stage 5's commit step, group commit and protocol server under the simulator's faults and process kills; Elle under faults on one node; scrith's Dgraph probes Q1 and A8 re-run as tests: a read rooted on an index every write sets, and a hub's reverse edge, each checked against every commit below its read timestamp | Elle finds no anomaly under any fault class; no acknowledged commit is lost across a kill; Q1 and A8 find no missed commit |
| **6. Query core** | What SPARQL and GQL share, as "The query core" sets out: the kernel (access patterns, operators, statistics) with binary joins planned from statistics; spargebra and the forked selene-gql as parsers; the core's register, which refuses a query outside the core naming the feature; spareval on the quad keyspaces through `QueryableDataset` as the SPARQL oracle; reads, CRUD writes through the changeset path, bounded walks, vector and text search, and catalog declarations through the binary protocol | The selected W3C SPARQL 1.1 tests and MGQL's golden results inside the core pass; no unexplained difference from spareval or MGQL there; one question asked in SPARQL and in GQL over the same data answers equally; the DeepBrain MVP's queries and scrith's catalogue reads run as scenarios |
| **7. Single-node product** | Access control and namespaces on the catalog; backup, restore and export; the live and bulk loaders, on quads, resolving every IRI in a dictionary pass first; the commit log; administration and metrics; client libraries and their documentation, the transaction limits included | The DeepBrain MVP runs against a single-node server through the Rust client; backup and restore round-trip under the simulator's faults; the dictionary's growth under the MVP's workload measured; the mapping contract at a stable version |
| **8. Full SPARQL and GQL conformance** | Everything outside the core: SPARQL's `OPTIONAL`, `UNION`, `MINUS`, full property paths, `CONSTRUCT`, `DESCRIBE`, `ASK`, graph management and the SPARQL Protocol gateway; GQL's `OPTIONAL MATCH`, path variables and path modes, shortest paths, set operations, `OTHERWISE` and the full Flagger and feature register; MGQL's exporter over its whole fragment, its theorem-based property tests and seed scenarios; the W3C suites in CI; worst-case-optimal joins and morsel parallelism; SPARQL 1.1 Service Description, VoID statistics and GQL catalog queries from the data dictionary; SHACL and GQL graph types as the catalog's surface syntaxes | The dual roadmap's phase-1 exit gate: the W3C SPARQL 1.1 query and update suites pass natively, the GQL scenarios and the openCypher TCK at an agreed rate, no unexplained difference from spareval or MGQL, published conformance numbers for SPARQL 1.1, the TCK and the GQL feature register; LDBC and SPARQL baselines |
| **9. Bolt** | The Bolt adapter, the limit's abort classified as transient; Bolt's schema procedures (`db.labels()`, `db.relationshipTypes()`, `db.propertyKeys()`, `db.schema.nodeTypeProperties()`, `db.schema.relTypeProperties()`, `db.schema.visualization()`, `dbms.components()`, the `SHOW` commands, an `apoc.meta.data()`-shaped result) with Neo4j's exact names, columns and value types, from the data dictionary | Neo4j drivers in two languages run the TCK through Bolt; LangChain's and LlamaIndex's Neo4j graph stores initialise against the server |
| **10. Distributed tiles** | First on one node, unreleased: read-only tiles built from the KV store, then tiles as a read path, with storage-differential testing. Then the ordering service over a VSR-replicated log; compute and storage nodes; tile placement, moves and rebalancing; membership; tile paging for graphs larger than memory; the multi-node deterministic simulator; Jepsen and Elle under faults | Every conformance suite gives identical results on the KV and the tile paths; Elle checks strict serializability under faults; a group failure loses no data; a graph larger than one node's memory runs the benchmark suites |

After stage 10: UltraQL, the dual roadmap's M9 and M10.

### Decided ahead, planned per stage

What crosses a stage boundary as an interface is decided ahead, because it is cheap to fix now and costly to
retrofit. Everything behind those interfaces is planned in its own stage.

| Item | Decided now | Planned in its stage |
|---|---|---|
| Storage and compute separation | The three roles above, as the target; stage 5 builds them in one process; Zero and Alpha are not built | Stage 10: deployment, placement, failure handling |
| VSR in place of Raft | The ordered-log interface (append a decided epoch, read in order, durable before acknowledgement), defined in stage 5 with a local implementation; Raft is not built | Stage 10: the VSR protocol, view changes, recovery |
| The binary protocol | Nothing in the engines or the server depends on gRPC or protobuf types; stage 4 removes Dgraph's API | Stage 5: the byte layout, the conformance suite, the client |
| Changesets, read summaries, region identity, UID type bits, commit records | Their shapes, so stage 10's tiles and log carry them unchanged | — |
| Tile format, rebalancing policy, the Jepsen suite | — | Stage 10 |

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
   together; the storage and concurrency work is the smaller part. Stage 6 builds only the query core, so the single
   node reaches the DeepBrain MVP before the engines are complete.
8. **The core's boundary.** A DeepBrain need that falls outside what SPARQL and GQL share either pulls a stage-8
   feature forward or is served by one language alone. Ranges over multi-valued predicates are the first such need,
   and SPARQL serves them ("Which language DeepBrain writes in").
6. **Stage 10 lands three things at once:** a new storage engine, sharding and replication. The unreleased single-node
   tile milestone, storage-differential testing and the multi-node deterministic simulator take it in steps.
7. **Read summaries' false aborts.** If coarse regions or filter false positives abort too many read-write
   transactions, validation falls back towards exact read sets and their cost; stage 5 measures the rate before stage
   10 depends on it.

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

### MERGE and WITH

The query core is ISO GQL, which has neither. The DeepBrain MVP's queries are written to it:
- **`MERGE`** becomes a `MATCH` and, where nothing matched, an `INSERT`, in one read-write transaction. Strict
  serializability makes the pair atomic. Two transactions inserting the same node conflict, because each read the
  index range the other wrote into, and the retry finds the node. A unique-value declaration refuses a duplicate
  whatever query wrote it.
- **`WITH`** becomes `NEXT`.

### Which language DeepBrain writes in

Both engines read one store, so DeepBrain writes each requirement in the language that states it best, and the dual
engine adds value from the MVP on. scrith's ontology backend generates every query in `Projection::plan`, so the
choice is made per question, in code.

| Requirement | Language | Why |
|---|---|---|
| A range over a multi-valued predicate: `time.periods`, `space.cells` | SPARQL | Each value is a triple of its own, so a `FILTER` range matches when any value does |
| Insert where absent, in one statement | SPARQL | `INSERT … WHERE { FILTER NOT EXISTS … }` is one update; in GQL it is a `MATCH` and an `INSERT` in one transaction |
| IRIs and external vocabularies: OWL-Time, SKOS, `org:`, GeoSPARQL | SPARQL | Terms are IRIs as the vocabularies write them |
| Walks over the organisation, concept and place trees | Either | `p+` and `p*` terminate because D46 bounds the tree's depth; GQL's `{m,n}` states the bound |
| The memory graph's patterns: `:Memory`, `:Entity`, `HAS_ENTITY`, `RELATION`, counts of shared entities | GQL | Node and edge patterns with labels and properties, as DeepBrain's MVP plan writes them |

### Datatypes stored by value

XSD's types and the system's vector datatype are stored by value. Storing a value pays where values are totally ordered
and have one canonical form; geometries have neither, and one polygon has several WKT forms. Geometries are stored by
their lexical form, as every other typed literal is, and indexed by Dgraph's `geo` tokenizer, which covers a shape with
S2 cells (`ultragraphdb_core_tok`, `ultragraphdb_core_types/src/s2`). That tokenizer reads GeoJSON, so
`geo:geoJSONLiteral` (GeoSPARQL 1.1, Requirements 25–27) is indexed from stage 4, and `geo:wktLiteral` from stage 8,
with a WKT parser and GeoSPARQL's functions. Until then, scrith answers an area from the S2 cells its writer derives
into `space.cells`.

Each tokenizer names the datatypes it takes and computes its tokens from the value, so `"042"^^xsd:integer` and `42`
index alike; a literal of another datatype is held unindexed (stage 4's index-maintenance spec).

### Which transactions read and write

A read-write transaction reads by key; traversals, retrieval and analytics run read-only, where they are never validated
and never abort. DeepBrain's writes are a memory with its entities and relations; its retrieval, vector search and one
or two hops, is read-only. scrith's largest read-write transaction is D46's batch of 1,000 member moves, whose
`Hierarchy` check walks a tree D46 bounds. The client documentation states the rule beside the transaction limits.

What costs validation is a read-then-write on a hot key, not the share of writes: inserts are blind and never
conflict, however many there are. DeepBrain's hot read-then-write is entity deduplication, the match-then-insert on a
popular entity. Stage 5 measures it over a Zipf-distributed set of entities at 8 writers, scrith's largest expected
number of gateway replicas, with and without the per-key path, and D46's batch beside it.

### How long an epoch lasts

On one node an epoch is a group commit, and it closes when the previous epoch's sync completes, with no timer. A lone
write then waits about one sync, and epochs grow with load. DeepBrain's writes sit beside language-model calls of
hundreds of milliseconds to seconds, so a few milliseconds per acknowledgement is noise. Fixed epochs of a few
milliseconds belong to stage 10's ordering service, which spans nodes.

### The commit log

A read-write transaction may attach one record, opaque to the store. The commit step writes the record in the
transaction's batch, keyed by namespace and commit timestamp, so it commits exactly when the transaction does and an
aborted transaction leaves none.
- **The cursor is a commit timestamp.** Reading after a cursor returns, in commit order, every record above it up to
  the visibility watermark. The watermark guarantees no commit later appears below it, so the read is complete with
  no gapless numbering.
- **A subscription answers at a snapshot** at the watermark and continues from that timestamp, so the answer and the
  records after it neither overlap nor leave a gap.
- **Retention** prunes records below a floor; a cursor below it is refused as unknown.
- **The record's content is the client's.** scrith's encoded `Commit` carries personal values by reference, never as
  copies (Gen4 D4-J), and the store never reads it. A feed of raw changesets would copy personal values out of reach of
  erasure, which is why the store carries records rather than changes.
- **Retries** are deduplicated by a unique value on the record's request identifier, which stage 4's catalog provides.
- **From stage 10** the records travel in the replicated ordered log.

The record costs one blind write per committing transaction. On UltraGraphDB it replaces Gen4's outbox numbering:
the cut, the sequencer and its lease, the epoch-rotated pending flags of D45, the wait of about a second for a number
and the stall when a sequencer fails over.

### What an acknowledgement promises

An acknowledged commit survives power loss. On one node, each epoch's batch is synced to disk before any of its
transactions is acknowledged; the batch shares one sync among its transactions, and stage 5 measures the sync's cost.
From stage 10, the replicated log takes this role: an epoch is acknowledged once a quorum of the ordering service holds
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
  lists, the uid codec and stage 10's tiles work on 64-bit UIDs, and a 64-bit hash would not do: by
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
- **Vectors** of the system datatype `ugdb:float32vector` are encoded by value at any length, as their dimension and
  little-endian 32-bit floats, and accept only their canonical lexical form, so term identity stays exact. They take no
  dictionary entry and no reverse or object-predicate entry. Hashed, a 768-element embedding would leave several KB in
  the dictionary for every write of it, and every HNSW distance would parse text.
- **Literal dictionary entries are written blind:** equal strings produce equal keys and equal values, so concurrent
  inserts commute and are left out of conflict checks.
- **Dictionary entries are not removed before stage 10.** A removal outside the commit step races a commit that writes
  the same entry again: if the removal lands after it, the new quad names a hash with no entry. A removal is therefore
  a commit of its own, ordered by the commit step and refused if any commit after its scan wrote the entry or a quad
  naming it. Stage 7 measures the dictionary's growth under the DeepBrain MVP's workload; that measurement decides
  whether removal moves to the single node, which stage 5's commit step already makes possible.
- **In a posting list** a literal object is a value posting whose value is its encoded term. The posting's 64-bit
  fingerprint orders the list, and ties compare the encoded terms, so a fingerprint collision never merges two values.

### The data dictionary

One read API, built in stage 4, answers what the data holds: labels, relationship types, property keys, per-label
shapes, declarations, indexes and statistics. Every interface answers from it: the describe request (stage 5), SPARQL
Service Description, VoID and GQL catalog queries (stage 8), and Bolt's schema procedures (stage 9). The procedures
matter to Bolt's first clients: LangChain's `Neo4jGraph` refreshes its schema in its constructor by default and fails on
`ProcedureNotFound`, asking for APOC and `apoc.meta.data()`; LlamaIndex's Neo4j store does the same.

- Declared entries answer exactly, from the catalog, where Neo4j samples data for `nodeTypeProperties`.
- Per-predicate object kinds and datatypes are exact by deltas, without reads.
- Per-label shapes of undeclared data come from a background pass over a snapshot, fresh within seconds, so writes stay
  read-free.

### The vector index

It stays, as an extension: SPARQL reaches it through an extension function, and GQL through a procedure call, in
stage 6's query core, where the DeepBrain MVP's `vector.search` needs it.
