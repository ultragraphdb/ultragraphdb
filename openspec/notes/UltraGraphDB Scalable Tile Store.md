# UltraGraphDB Scalable Tile Store

Nov 9, 2026 · @Marvin Hansen

## Summary

The tile store is UltraGraphDB's read-optimised graph storage: an immutable CSR form that multi-hop queries and graph
algorithms read at peak speed. It is designed from scratch through a staged design process with reviews, starting with a
survey of open-source graph stores with proven performance. The tile design in
`UltraGraphDB Roadmap Dual Query Engine and Tile Storage.md` is source material, not a starting point.

This note collects the research plan and the questions the design has to answer.

## Requirements before the survey

The requirements are written first, so the first system surveyed does not become the template. At minimum:

- **Workloads:** ontology writes; 3–5 hop reads, including from hubs; graph algorithms.
- **Algorithms, by name.** The list decides the layout:
  - PageRank pulls along incoming edges and needs the reverse (CSC) index.
  - Breadth-first search switches between pushing along outgoing edges and pulling along incoming ones.
  - Weighted shortest paths need edge weights stored next to the adjacency.
- **Isolation:** the level every read path keeps.
- **Memory:** whether the graph must fit in memory.
- **Graph shapes:** hypergraphs and multigraphs, which none of the surveyed systems handle natively.

## Survey

### Sources

Systems with readable source and published designs:

| System | Licence | Of interest |
|---|---|---|
| TuGraph-DB (Ant Group) | Apache 2.0 | Has held audited LDBC SNB Interactive records |
| GraphScope (Alibaba) | Apache 2.0 | GART, a CSR that accepts writes; its own LDBC results |
| Kuzu | MIT (archived, readable) | Columnar CSR with node groups, multi-version reads, a write-ahead log |
| ArcadeDB | Apache 2.0 | Its storage format; its default isolation and fsync policy |
| LiveGraph, Sortledton, Teseo, GTX, Aspen | Research systems | Each designed for CSR under writes |

### LDBC audits as the yardstick for isolation

An audited LDBC SNB Interactive result must pass the benchmark's ACID tests, and the audit report states the isolation
level used. Systems with audited results have documented their isolation; systems publishing headline numbers alone have
not. The same yardstick applies to UltraGraphDB's own claims later.

### ArcadeDB: measure, don't infer

The Apache 2.0 licence permits measuring instead of reading between the lines:

- Run ArcadeDB's benchmark with and without the serializable flag, and record the drop.
- Read the default fsync policy in the source. That settles the power-loss claim with evidence.

## The dual representation

### Starting point

The DeepCausality UltraGraph library, which processes in memory, uses two representations:

- **Mutable:** takes writes, and can be serialised, which solves writing arbitrarily complex hyper-multigraphs to disk.
- **Frozen:** an immutable CSR built by a freeze operation. The algorithms need it to run at peak performance.

For the database, the idea is to rebuild the frozen CSR segment by segment after each batch commits. Turning this into a
workable storage format is open, and it may never work, but it is a starting point.

### A known database pattern

A mutable store for writes plus an immutable store rebuilt for reads is the delta/main design: C-Store's writeable and
read-optimised stores, SAP HANA's delta merge. The pattern has a track record. What is open is turning a whole-graph
freeze into a freeze per segment.

### Questions the design must answer

**What is a segment?**
If it is a node range, one edge u→v dirties u's outgoing segment and v's incoming segment, which may be different
segments. In a hypergraph stored per role, one hyperedge dirties a segment for every node it joins.

**What do algorithms read?**
A strictly serializable read at time T sees the frozen state plus every change since the freeze. If algorithms read only
the frozen state for speed, they read the past. That is acceptable only as a stated semantic, an as-of read at the
freeze epoch, never as a side effect. Do algorithms merge the recent changes, or read at the last freeze by definition?

**What starts a freeze, and what does it cost?**
In memory, a freeze rebuilds all edges at once. A database needs it per segment, triggered by the volume of changes,
running alongside reads, and keeping the old frozen version until no snapshot reads it any more.

**Is only the mutable side durable?**
Both representations would otherwise need replication, backups and recovery. If the frozen side can always be rebuilt
from the mutable side, it is a cache:

- Recovery stays simple: only the mutable side is durable state.
- A bug in the frozen side returns wrong reads but loses no data, and a rebuild repairs it.
- Reads from the frozen side can be compared against reads from the mutable side, a test oracle for the whole tile store.

Does the design keep that property?

### Lessons learned from fjal KV store

@TODOL Add from the open session before closing the specs. 