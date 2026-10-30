# Roadmap: UltraGraphDB


| Stage | Status | 
|---|---|
| 1. Foundations + shared types | ✅ |
| 1.5. Fault simulator | ✅ | 
| 2. Storage core |  ✅ | 
| 3. Website + Public Repo |  ✅ |
| 4. Data model and quad storage | ✅ |
| 5. Transactions, WAL, and the zero copy binary protocol | `in progress` |
| 5.5. Hardening Transactions with chaos testing | `open` |
| 6. Query core: SPARQL and GQL where they overlap | `open` |
| 7. Single-node server, the base of the DeepBrain MVP | `open` |
| 8. Full SPARQL and GQL conformance | `open` |
| 9. Bolt | `open` |
| 10. Distributed tiles: sharding, replication, storage/compute separation | `open` |

Stages 4 to 10 replace DQL with SPARQL and GQL; the plan is `openspec/notes/UltraGraphDB Dual Query Engine Plan.md`.
UltraQL follows stage 10.

The DeepBrain MVP runs on the single node of stage 7. Stage 6 builds only what SPARQL and GQL share: CRUD, pattern
matching, filters, ordering, aggregates, bounded walks over one relation, and vector and text search. Where the two
languages diverge, and full conformance testing, follow in stage 8; Bolt follows in stage 9.

Fuzzing in stage 6 centres on the query core, where it pays most: the SPARQL and GQL parsers and the planner take text
from any client, and a query language's grammar nests without limit. Each parser gets a structure-aware fuzzer that
builds queries from the grammar, seeded with the conformance suites' queries, so runs reach the planner rather than
stopping at a syntax error; every nesting is bounded by a stated depth, and unbounded recursion is a finding. Where the
two languages overlap, a query and its translation into the other language must return the same rows, which turns
fuzzing into a differential test of both engines.

Improvements, single node:

1. Add a fault-injecting simulator to stress test and harden the storage engine! (done, stage 2)
2. Replace proto / gRPC with a proper, zero copy, binary protocol in a dedicated crate with conformance protocol! (stage 5)
   - Re-implent client and single node against the binary protocol (stage 5)
3. Explore a thread-per-core architecture to speed up read queries per second (QPS) and route mutateion queries to a single writer thread (Tigerbeetle) (stage 5: n−1 worker cores and one commit step)

--

Improvements, clustering:
1) Replace Raft with the more efficient VSR protocol (stage 10; Raft is not built, stage 5 defines the log interface)
2) Add Jepsen test suite to harden clustering (stage 10)
3) Replace the Zero/Alpha seperation with a proper storage/compute seperation (target decided now, built in stage 10; Zero/Alpha is not built)
4) Adopt Aeron's stream position for the replicated log (stage 10). Aeron locates every frame by its byte offset in the
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

storage/compute seperation
https://github.com/starrocks/starrocks

--

Benchmarking:
tigergraph
https://www.tigergraph.com/benchmark/
https://github.com/tigergraph/ecosys/tree/ldbc/ldbc_benchmark/tigergraph/queries_v3

nebula
https://nebula-graph.io/posts/benchmarking-the-mainstream-open-source-distributed-graph-databases-at-meituan-nebulagraph-vs-dgraph-vs-janusgraph





