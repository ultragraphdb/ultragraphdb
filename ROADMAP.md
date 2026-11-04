# Roadmap: UltraGraphDB

| Stage | Status |
|---|---|
| 1. Foundations and shared types | ✅ |
| 1.5. Fault simulator | ✅ |
| 2. Storage core | ✅ |
| 3. Website and public repository | ✅ |
| 4. Data model and quad storage | ✅ |
| 5. Transactions, WAL and the zero-copy binary protocol | ✅ |
| 5.5. Refactor to prepare for the three-role cluster (frontend, compute, storage) | `in progress` |
| 6. UGS: the full data definition and its implementation | `open` |
| 6.5. Harden transactions  with chaos testing against multiple UGS schemata | `open` |
| 7. Query core: SPARQL and GQL where they overlap | `open` |
| 7.5. Fuzzy test and harden query core | `open` |
| 8. Basic three-role clustering with VSR replication in a single container, the base of the DeepBrain MVP | `open` |
| 9. Full SPARQL and GQL conformance | `open` |
| 9.5. Bolt | `open` |
| 10. Full distributed clustering and tiles: sharding, replication at scale, storage/compute separation | `open` |

UltraGraphDB is distributed from the start: three roles, frontend, compute and storage, from one binary, replicated
with VSR (`openspec/notes/UltraGraphDB Distribution Architecture.md`, and for the protocol itself
`openspec/notes/UltraGraphDB Distributio Protocol.md`). A single machine runs the same topology folded
into one container (`openspec/notes/UltraGraphDB Single-Container Deployment.md`). Stages 4 to 10 replace DQL with
SPARQL and GQL; UltraQL follows stage 10. What the next stages must specify is tracked in `openspec/notes/backlog.md`.

- **5.5 Refactor for the three-role cluster.** The seams the cluster needs before UGS and clustering build on them: the
  128-bit system ID with its type prefix and bucket; the placement function; the commit step as one replica group's
  state machine, its rounds stamped with epochs; the metadata group's state machine with one replica, the catalog's
  home; the stage-5 crates sorted into the roles; the transport's in-process carrier.
- **6 UGS.** The schema language (`openspec/notes/UltraGraphDB Schema and Query Language.md`), specified first as
  openspec capabilities. Stage 6 builds the modelling core, local constraints and the RDF view; it parses and validates
  the time, bitemporal, geometry, unit, fold, function and module constructs; what needs groups or change streams
  waits for stage 8.
- **7 Query core.** Only what SPARQL and GQL share: CRUD, pattern matching, filters, ordering, aggregates, bounded walks
  over one relation, and vector and text search. Where the two languages diverge, and full conformance testing, follow
  in stage 9. The design is `openspec/notes/UltraGraphDB Dual Query Engine Plan.md`, "The query core".
- **8 Basic three-role clustering in a single container.** Frontend, compute and storage, the metadata group and the
  sequencer in one container, two storage groups by default; VSR's first build, its groups of three replicas running on
  the `dev-ha` profile, proved and simulated acknowledging from memory; epochs, change streams, Calvin ordering for
  multi-group transactions, write classification from the schema. The DeepBrain MVP runs here, on the single container: the first
  stage whose topology it can run on, since no role runs alone.
- **9 Full conformance.** Everything outside the core, the W3C suites, the openCypher TCK and the GQL feature register.
- **9.5 Bolt.** The Bolt adapter and Neo4j's schema procedures; Neo4j drivers run the TCK through it.
- **10 Full distribution and tiles.** Clustering at scale, Jepsen, tiles and storage/compute separation across nodes. Stage
  10 opens by measuring replication on physical Linux nodes, which sets the acknowledgement's default and decides CURP's
  fast path ("Experiments" in the distribution protocol note).

Fuzzing in stage 7 centres on the query core, where it pays most: the SPARQL and GQL parsers and the planner take text
from any client, and a query language's grammar nests without limit. Each parser gets a structure-aware fuzzer that
builds queries from the grammar, seeded with the conformance suites' queries, so runs reach the planner rather than
stopping at a syntax error; every nesting is bounded by a stated depth, and unbounded recursion is a finding. Where the
two languages overlap, a query and its translation into the other language must return the same rows, which turns
fuzzing into a differential test of both engines.

Improvements, done:

1. A fault-injecting simulator to stress test and harden the storage engine (stage 2)
2. A zero-copy binary protocol in crates of its own, with a conformance suite, replacing protobuf and gRPC; the client
   re-implemented against it (stage 5)
3. One commit step per log, after TigerBeetle's single writer (stage 5); the threads follow the roles, thread per core

--

Improvements, clustering:
1) Replace Raft with the more efficient VSR protocol (stage 8 for the basic cluster, stage 10 at full scale; Raft is not built, stage 5 defines the log interface). The protocol, acknowledging from memory with a bounded exposure, is `openspec/notes/UltraGraphDB Distributio Protocol.md`
2) Add Jepsen test suite to harden clustering (stage 10)
3) Replace the Zero/Alpha separation with a proper storage/compute separation (built in stage 8 as the three roles, scaled out in stage 10; Zero/Alpha is not built)
4) Adopt Aeron's stream position for the replicated log (stage 8, with VSR). Aeron locates every frame by its byte offset in the
   stream, so a receiver writes each frame at its offset in whatever order it arrives: a duplicate rewrites the same
   bytes, a gap is a hole below the highest position seen and is repaired by a NAK naming the range, and the same
   number drives flow control. A replica catching up then seeks into the log by position, with no index. The client
   connection keeps its frame count: TCP delivers in order, so `SequenceOrder` there is a tripwire, not a recovery.

   Open questions for the design stage:
   - Does the Round frame carry the byte position, the round count (VSR's op number), or both, and which one does the
     header's `sequence` hold?
   - Does replication run over TCP, where frames arrive in order, or over a transport that loses and reorders (UDP,
     multicast, several channels), where writing at the offset earns its keep?
   - Does a follower repair a gap by a NAK naming (position, length), and does the leader's heartbeat carry its highest
     position so a follower detects tail loss?
   - Does leader-to-follower flow control run on positions, a consumption position plus a byte window, as Aeron's
     status messages do?
   - After a view change truncates a follower's uncommitted suffix, the new leader's log may hold other bytes at the
     same position: do the position and the `parent` checksum chain together decide where two logs diverge?
   - Do positions run on across a new `incarnation`, or restart, as Aeron starts each stream at a random term id?
   - Do log segments take a power-of-two size, so a position maps to (segment, offset) by shift and mask, and is a Round
     frame that does not fit at a segment's end padded, as Aeron pads a term?
   - Does a snapshot record its log position, so a replica restores it and replays from there?
   - How do Aeron Archive and Aeron Cluster define their recording and log positions? Their documentation is unread so
     far; the transport, flow-control and data-structure wiki pages are the source of the above.

storage/compute separation
https://github.com/starrocks/starrocks

--

Benchmarking:
tigergraph
https://www.tigergraph.com/benchmark/
https://github.com/tigergraph/ecosys/tree/ldbc/ldbc_benchmark/tigergraph/queries_v3

nebula
https://nebula-graph.io/posts/benchmarking-the-mainstream-open-source-distributed-graph-databases-at-meituan-nebulagraph-vs-dgraph-vs-janusgraph





