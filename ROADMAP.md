

| Stage | Status | 
|---|---|
| 1. Foundations | ✅ |
| 1.5. Fault simulator | `Next` | 
| 2. Storage | `open` | 
| 2.5. Harden Storage | `open` | 
| 3. Single node | `open` |
| 4. Replication | `open` | 
| 5. Sharding and operations | `open` |

Improvements, single node:

1. Add a fault-injecting simulator to stress test and harden the storage engine!
2. Replace proto / gRPC with a proper, zero copy, binary protocol in a dedicated crate with conformance protocol!
   - Re-implent client and single node against the binary protocol
3. Explore a thread-per-core architecture to speed up read queries per second (QPS) and route mutateion queries to a single writer thread (Tigerbeetle) 

--

Improvements, clustering:
1) Replace Raft with the more efficient VSR protocol
2) Add Jepsen test suite to harden clustering 
3) Replace the Zero/Alpha seperation with a proper storage/compute seperation 

storage/compute seperation
https://github.com/starrocks/starrocks

--

Benchmarking:
tigergraph
https://www.tigergraph.com/benchmark/
https://github.com/tigergraph/ecosys/tree/ldbc/ldbc_benchmark/tigergraph/queries_v3

nebula
https://nebula-graph.io/posts/benchmarking-the-mainstream-open-source-distributed-graph-databases-at-meituan-nebulagraph-vs-dgraph-vs-janusgraph





