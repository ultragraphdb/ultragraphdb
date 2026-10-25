| Stage | Status | 
|---|---|
| 1. Foundations | ✅ |
| 1.5. Fault simulator | ✅ | 
| 2. Storage |  ✅ | 
| 3. Website + Public Repo |  ✅ |
| 4. Data model and quad storage | `next` |
| 5. Transactions and the binary protocol | `open` |
| 6. SPARQL engine | `open` |
| 7. GQL engine and Bolt | `open` |
| 8. Single-node product | `open` |
| 9. Distributed tiles: sharding, replication, storage/compute separation | `open` |

Stages 4 to 9 replace DQL with SPARQL and GQL; the plan is `openspec/notes/UltraGraphDB Dual Query Engine Plan.md`.
UltraQL follows stage 9.

Improvements, single node:

1. Add a fault-injecting simulator to stress test and harden the storage engine! (done, stage 2)
2. Replace proto / gRPC with a proper, zero copy, binary protocol in a dedicated crate with conformance protocol! (stage 5)
   - Re-implent client and single node against the binary protocol (stage 5)
3. Explore a thread-per-core architecture to speed up read queries per second (QPS) and route mutateion queries to a single writer thread (Tigerbeetle) (stage 5: n−1 worker cores and one commit step)

--

Improvements, clustering:
1) Replace Raft with the more efficient VSR protocol (stage 9; Raft is not built, stage 5 defines the log interface)
2) Add Jepsen test suite to harden clustering (stage 9)
3) Replace the Zero/Alpha seperation with a proper storage/compute seperation (target decided now, built in stage 9; Zero/Alpha is not built)

storage/compute seperation
https://github.com/starrocks/starrocks

--

Benchmarking:
tigergraph
https://www.tigergraph.com/benchmark/
https://github.com/tigergraph/ecosys/tree/ldbc/ldbc_benchmark/tigergraph/queries_v3

nebula
https://nebula-graph.io/posts/benchmarking-the-mainstream-open-source-distributed-graph-databases-at-meituan-nebulagraph-vs-dgraph-vs-janusgraph





