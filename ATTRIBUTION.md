# Attribution

UltraGraphDB is built on ideas from other databases, research systems and standards. This file names each
high-level concept the design takes from another project and the project it comes from, then lists every
named project with its website and its licence as stated on 2026-11-02.

The concepts come from the design notes in `openspec/notes/`. Each is marked with one of two kinds:

- **Borrowed**: the design adopts the concept or mechanism, or ports the code that implements it.
- **Inspired**: the design draws on the concept and changes it.

Code ported from Dgraph and its Go dependencies keeps the copyright line of its source file and states that
it was changed, as `AGENTS.md` sets out under *Ported code and attribution*. The libraries the workspace
links are listed with their versions in `AGENTS.md` under *External dependencies*.

## Notes cited

| Key  | Note in `openspec/notes/`                                   |
|------|-------------------------------------------------------------|
| DIST | `UltraGraphDB Distribution Architecture.md`                 |
| SPC  | `UltraGraphDB Shard-per-Core Write Path.md`                 |
| SQL  | `UltraGraphDB Schema and Query Language.md`                 |
| DQE  | `UltraGraphDB Dual Query Engine Plan.md`                    |
| RM   | `UltraGraphDB Roadmap Dual Query Engine and Tile Storage.md` |
| ACP  | `UltraGraphDB Agentic Context Projection.md`                |
| SCD  | `UltraGraphDB Single-Container Deployment.md`               |
| SEC  | `security.md`                                               |
| PERF | `perf-optimization.md`                                      |
| BL   | `backlog.md`                                                |
| PORT | `archive/dgraph-port-overview.md`                           |
| BUG  | `archive/dgraph-bugs-found-in-port.md`                      |
| NC   | `archive/native-clients.md`                                 |

## Index

Each row gives a concept, the slug of the project it comes from, and the kind of attribution. The slugs
resolve in [Projects](#projects), [Publications](#publications) and [Standards](#standards).

### Lineage, storage and encoding

| Slug             | Concept                                                                                                                                                                                                                     | Kind     | Notes     |
|------------------|-----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|----------|-----------|
| `dgraph`         | The codebase the project was ported from: posting lists and their encoding, rollups and split lists, index maintenance, rebuilds and drops, index tokenizers, types and schema, the HNSW vector index, access control and namespaces, backup, restore and export, the bulk and live loaders | Borrowed | PORT      |
| `dgraph`         | Value-index keys that hold subject IDs                                                                                                                                                                                      | Borrowed | DQE       |
| `dgraph`         | Identifiers resolved in a dictionary pass before quads are written, as Dgraph's bulk loader assigns UIDs first                                                                                                             | Borrowed | DQE       |
| `dgraph`         | A batch's requests applied in order                                                                                                                                                                                         | Borrowed | PERF      |
| `dgraph`         | Shape-preserving tree results, from DQL, in UltraQL                                                                                                                                                                         | Borrowed | RM        |
| `dgraph-rs`      | The Rust client, copied and renamed as `ultragraphdb_client`                                                                                                                                                                | Borrowed | PORT      |
| `badger`         | A store without transactions of its own, the outcome decided above it (Badger's managed mode)                                                                                                                               | Borrowed | PORT      |
| `badger`         | The commit timestamp kept in the key, newest version first; compaction drops versions below the discard timestamp                                                                                                          | Borrowed | PORT, PERF |
| `go`             | `encoding/json` decoding semantics, ported; float arithmetic without fused multiply-add, as amd64 Go computes; `errors.Join` text                                                                                          | Borrowed | PORT, BUG |
| `s2`             | Spatial index on S2 cell coverings                                                                                                                                                                                          | Borrowed | SQL, DQE  |
| `golang-geo`     | S2 geometry and the geo index tokens, ported from the Go implementation of S2                                                                                                                                               | Borrowed | PORT      |
| `go-geom`        | Geometry types, ported                                                                                                                                                                                                      | Borrowed | PORT      |
| `bleve`          | Text analysis: tokenizers and token filters, ported                                                                                                                                                                         | Borrowed | PORT, BUG |
| `bleve-segment`  | Unicode word segmentation, ported                                                                                                                                                                                           | Borrowed | BUG       |
| `go-farm`        | The `Fingerprint64` hash, ported                                                                                                                                                                                            | Borrowed | PORT      |
| `go-groupvarint` | Group-varint integer encoding, ported                                                                                                                                                                                       | Borrowed | PORT      |
| `oxigraph`       | RDF term encoding: short strings inline, longer ones hashed, the dictionary filled by insert-if-absent                                                                                                                       | Borrowed | DQE       |

### Transactions and ordering

| Slug           | Concept                                                                                                                                                  | Kind     | Notes               |
|----------------|----------------------------------------------------------------------------------------------------------------------------------------------------------|----------|---------------------|
| `foundationdb` | Ordering and acknowledgement for strict serializability: the read-version and commit-version rules, run inside one process                              | Borrowed | DQE                 |
| `foundationdb` | Read-set validation at commit; the resolver's batch conflict check, in which an aborted transaction adds no writes                                       | Borrowed | PORT, DQE           |
| `calvin`       | Deterministic sequencing of cross-group transactions in epochs, without two-phase commit; partitioned sequencers merged deterministically per epoch       | Borrowed | DIST, SPC, DQE, BL  |
| `calvin`       | Participants exchange their reads; a dependent transaction takes a reconnaissance read, then a sequenced check that its read set is unchanged            | Borrowed | SPC, SQL            |
| `tigerbeetle`  | Serial execution of server-side conditions                                                                                                               | Borrowed | PORT                |
| `ramp`         | Blind writes across partitions made atomic at an epoch (read-atomic multi-partition transactions)                                                        | Borrowed | SPC                 |
| `ramp-tao`     | Read atomicity built in, as watermarks                                                                                                                   | Inspired | DIST                |
| `neo4j`        | The transaction time limit exposed as a setting, as `db.transaction.timeout` is                                                                          | Borrowed | DQE                 |
| `tigergraph`   | A 16 s default query timeout                                                                                                                             | Borrowed | DQE                 |

### Replication, partitioning and cluster

| Slug                      | Concept                                                                                                                                                         | Kind     | Notes                |
|---------------------------|-----------------------------------------------------------------------------------------------------------------------------------------------------------------|----------|----------------------|
| `vsr`                     | Viewstamped Replication for replica groups: storage, metadata and the sequencer                                                                                 | Borrowed | DIST, SPC, SCD, DQE  |
| `raft`                    | Idle groups quiesce, heartbeats coalesce and leaders spread across nodes, as multi-Raft systems do                                                              | Borrowed | SPC                  |
| `scylladb`                | Shard-per-core: one shared-nothing partition per core                                                                                                           | Borrowed | SPC, BL              |
| `tigerbeetle`             | One single-threaded core per replica                                                                                                                            | Borrowed | SPC, BL              |
| `tigerbeetle`             | Request batching to amortise consensus                                                                                                                          | Inspired | DIST                 |
| `tigerbeetle`             | Releases that bundle earlier state-machine versions, for rolling upgrades                                                                                       | Inspired | DIST                 |
| `couchbase`               | A fixed bucket count whose buckets move whole (vBuckets)                                                                                                        | Inspired | SPC                  |
| `powergraph`              | Vertex-cut: a high-degree node's reverse list split across partitions                                                                                           | Borrowed | SPC                  |
| `starrocks`               | The split into frontend and compute nodes                                                                                                                       | Borrowed | DIST                 |
| `starrocks`               | The metadata state machine replicated on the frontends: leader, followers and non-voting observers                                                              | Borrowed | DIST                 |
| `starrocks`               | Vectorised batches, colocate joins, global runtime filters and low-cardinality dictionaries, each mapped to a graph equivalent                                 | Inspired | DIST                 |
| `starrocks`               | Rolling-upgrade order (compute before frontends, followers before the leader) and graceful exit                                                                 | Inspired | DIST                 |
| `cascades`                | A cost-based planner with distribution strategies as physical choices                                                                                           | Inspired | DIST                 |
| `compartmentalized-paxos` | Protocol roles (batching, ordering, execution) scaled independently, applied to VSR                                                                             | Inspired | DIST                 |
| `query-rewrites`          | Decoupling and partitioning of protocol roles                                                                                                                   | Inspired | DIST                 |
| `lupine`                  | A minimal Linux kernel by configuration specialisation                                                                                                          | Borrowed | DIST                 |

### Coordination avoidance

| Slug                   | Concept                                                                                                  | Kind     | Notes     |
|------------------------|----------------------------------------------------------------------------------------------------------|----------|-----------|
| `invariant-confluence` | Invariant confluence as the criterion for a coordination-free write; every write classified              | Borrowed | DIST, SQL |
| `hamsaz`               | A conflict graph over writes and constraints, computed when the schema is compiled                       | Borrowed | DIST, SQL |
| `lucy`                 | Segmented confluence: writes segmented by constraint ownership                                           | Borrowed | DIST      |
| `keep-calm`            | Query classification by monotonicity (CALM): monotonic reads served fast, the rest from a snapshot       | Borrowed | DIST, SQL |
| `homeostasis`          | Escrow treaties: a bounded numeric constraint split into per-group shares (`@limit`)                     | Borrowed | DIST      |
| `ipa`                  | Declared delete semantics: cascade, restrict, orphan to tombstone                                        | Inspired | DIST      |
| `redblue`              | Red and blue operation labels, derived from the schema instead of assigned by hand                       | Inspired | DIST      |

### Query, schema, storage layout and retrieval

| Slug          | Concept                                                                                                                  | Kind     | Notes     |
|---------------|--------------------------------------------------------------------------------------------------------------------------|----------|-----------|
| `typedb`      | The schema model of UltraQL: n-ary relations with roles, first-class attributes, abstract types and inheritance          | Borrowed | RM        |
| `rust`        | Type inference, and compiler diagnostics that give a location, the reason and a fix                                      | Inspired | RM, SQL   |
| `tigergraph`  | A query split into morsels by subject ID range, from TigerGraph's segments                                               | Borrowed | DQE       |
| `tigergraph`  | Partial results combined only by commutative merges at barriers, as TigerGraph's accumulators combine them               | Borrowed | DQE       |
| `gridgraph`   | A two-level grid of tiles; a traversal skips tiles with no active vertex                                                 | Borrowed | RM        |
| `combblas`    | Matrix tiles distributed over a 2D grid                                                                                  | Borrowed | RM        |
| `graphblas`   | 2D tile distribution and traversal as matrix-vector products over semirings                                              | Borrowed | RM        |
| `postgresql`  | Remote sources modelled on foreign data wrappers (`postgres_fdw`), with filters, columns and limits pushed down          | Borrowed | ACP       |
| `oracle-fdw`  | The same foreign-data-wrapper model, for Oracle sources                                                                  | Borrowed | ACP       |
| `allen`       | Allen's thirteen interval relations, as UQL operators                                                                    | Borrowed | SQL       |
| `bm25`        | BM25 full-text ranking                                                                                                   | Borrowed | SQL, ACP  |
| `rrf`         | Reciprocal rank fusion of text and vector results                                                                        | Borrowed | SQL       |

### Wire protocol and clients

| Slug          | Concept                                                                                                        | Kind     | Notes          |
|---------------|----------------------------------------------------------------------------------------------------------------|----------|----------------|
| `iggy`        | A byte-level binary protocol: fixed headers, index and data vectors                                            | Borrowed | RM, DIST       |
| `arrow`       | The columnar result buffer layout, which a client wraps as Arrow without copying                               | Borrowed | RM, SQL, DIST  |
| `kafka`       | A language-neutral message schema, generated codecs, and hand-written connection and client logic              | Borrowed | NC             |
| `sans-io`     | Protocol and client cores that perform no I/O                                                                  | Borrowed | NC, SCD        |
| `neo4j`       | The Bolt adapter, and Neo4j's schema procedures under their exact names, columns and value types               | Borrowed | DQE, PORT      |

### Security

| Slug                 | Concept                                                                                                                                                         | Kind     | Notes |
|----------------------|-----------------------------------------------------------------------------------------------------------------------------------------------------------------|----------|-------|
| `postgresql`         | The SASL exchange in the startup, SCRAM-SHA-256 with channel binding, `OAUTHBEARER` for single sign-on, roles holding roles, allow-only privileges, rows a principal may not read hidden, `authentication_timeout` | Borrowed | SEC   |
| `mongodb`            | SCRAM in the handshake; a privilege as an action on a resource; localhost only until authentication is configured; identity-provider groups mapped to roles     | Borrowed | SEC   |
| `neo4j`              | Fine-grained graph privileges; data a principal may not read made invisible instead of refused; Bolt `LOGOFF` and `LOGON`                                       | Borrowed | SEC   |
| `kafka`              | `OAUTHBEARER`, and re-authentication on an open connection (KIP-368)                                                                                            | Borrowed | SEC   |
| `foundationdb`       | Authorisation by signed tokens verified against trusted public keys and scoped to tenants                                                                      | Borrowed | SEC   |
| `redis`              | An unauthenticated server bound to loopback, as protected mode does                                                                                            | Inspired | SEC   |
| `dgraph`             | Predicate-level privileges, namespaces as tenants, a principal with its authentication method, a capability check that fails closed; tokens pinned to the configured algorithm, a required `exp`, constant-time comparison, a uniform login failure | Borrowed | SEC   |
| `nist-rbac`          | Hierarchical role-based access control: a principal's privileges are the union over its role graph                                                             | Borrowed | SEC   |
| `saltzer-schroeder`  | Fail-safe defaults, complete mediation, least privilege, separation of privilege                                                                               | Borrowed | SEC   |

### Testing

| Slug       | Concept                                                                                                                   | Kind     | Notes |
|------------|---------------------------------------------------------------------------------------------------------------------------|----------|-------|
| `mgql`     | A machine-checked reference semantics for GQL: golden results, theorems run as property tests, a certified type checker   | Borrowed | DQE   |
| `selene-db`| A machine-readable register of the optional GQL features an engine claims                                                 | Borrowed | RM    |
| `gdbmeter` | Metamorphic testing by ternary logic partitioning                                                                         | Borrowed | RM    |
| `gamera`   | Metamorphic testing by graph-aware data mutations                                                                         | Borrowed | RM    |

## Projects

Licences as each project states them on 2026-11-02, read from the project's licence file or licence page.

| Slug             | Project                                | Website                                          | Licence                                                                                                                                                   |
|------------------|----------------------------------------|--------------------------------------------------|-----------------------------------------------------------------------------------------------------------------------------------------------------------|
| `arrow`          | Apache Arrow                           | https://arrow.apache.org                         | Apache-2.0                                                                                                                                                |
| `badger`         | Badger                                 | https://github.com/dgraph-io/badger              | Apache-2.0                                                                                                                                                |
| `bleve`          | Bleve                                  | https://blevesearch.com                          | Apache-2.0                                                                                                                                                |
| `bleve-segment`  | blevesearch/segment                    | https://github.com/blevesearch/segment           | Apache-2.0                                                                                                                                                |
| `combblas`       | Combinatorial BLAS (CombBLAS)          | https://github.com/PASSIONLab/CombBLAS           | BSD-3-Clause-LBNL (BSD 3-clause with Lawrence Berkeley National Laboratory's enhancements grant)                                                          |
| `couchbase`      | Couchbase Server                       | https://www.couchbase.com                        | Source: Business Source License 1.1, changing to Apache-2.0 four years after each release. Packaged Community and Enterprise editions: Couchbase commercial licences |
| `dgraph`         | Dgraph                                 | https://dgraph.io                                | Apache-2.0 (© Istari Digital, Inc.)                                                                                                                       |
| `dgraph-rs`      | dgraph-rs                              | https://github.com/marvin-hansen/dgraph-rs       | MIT                                                                                                                                                       |
| `foundationdb`   | FoundationDB                           | https://www.foundationdb.org                     | Apache-2.0                                                                                                                                                |
| `gamera`         | Gamera                                 | https://github.com/cuhk-seclab/Gamera            | None stated in the repository                                                                                                                             |
| `gdbmeter`       | GDBMeter                               | https://github.com/gdbmeter/gdbmeter             | MIT                                                                                                                                                       |
| `go`             | The Go programming language            | https://go.dev                                   | BSD-3-Clause                                                                                                                                              |
| `go-farm`        | go-farm                                | https://github.com/dgryski/go-farm               | MIT                                                                                                                                                       |
| `go-geom`        | go-geom                                | https://github.com/twpayne/go-geom               | BSD-2-Clause                                                                                                                                              |
| `go-groupvarint` | go-groupvarint                         | https://github.com/dgryski/go-groupvarint        | MIT                                                                                                                                                       |
| `golang-geo`     | golang/geo (S2 for Go)                 | https://github.com/golang/geo                    | Apache-2.0                                                                                                                                                |
| `graphblas`      | GraphBLAS                              | https://graphblas.org                            | A C API specification; its reference implementation, SuiteSparse:GraphBLAS, is Apache-2.0                                                                 |
| `gridgraph`      | GridGraph                              | https://github.com/thu-pacman/GridGraph          | Apache-2.0                                                                                                                                                |
| `iggy`           | Apache Iggy                            | https://iggy.apache.org                          | Apache-2.0                                                                                                                                                |
| `kafka`          | Apache Kafka                           | https://kafka.apache.org                         | Apache-2.0                                                                                                                                                |
| `mgql`           | MGQL                                   | https://github.com/EngineeringSoftware/mgql      | MIT                                                                                                                                                       |
| `mongodb`        | MongoDB                                | https://www.mongodb.com                          | Community Server: Server Side Public License v1 (SSPL-1.0)                                                                                                |
| `neo4j`          | Neo4j                                  | https://neo4j.com                                | Community Edition: GPL-3.0. Enterprise Edition: Neo4j commercial licence                                                                                  |
| `oracle-fdw`     | oracle_fdw                             | https://github.com/laurenz/oracle_fdw            | PostgreSQL Licence                                                                                                                                        |
| `oxigraph`       | Oxigraph                               | https://oxigraph.org                             | MIT OR Apache-2.0                                                                                                                                         |
| `postgresql`     | PostgreSQL                             | https://www.postgresql.org                       | PostgreSQL Licence                                                                                                                                        |
| `powergraph`     | PowerGraph                             | https://github.com/jegonzal/PowerGraph           | Apache-2.0                                                                                                                                                |
| `redis`          | Redis                                  | https://redis.io                                 | Redis 8.0 and later: the user's choice of RSALv2, SSPLv1 or AGPL-3.0. Redis 7.2 and earlier: BSD-3-Clause                                                 |
| `rust`           | The Rust programming language          | https://www.rust-lang.org                        | MIT OR Apache-2.0                                                                                                                                         |
| `s2`             | S2 Geometry                            | https://s2geometry.io                            | Apache-2.0                                                                                                                                                |
| `scylladb`       | ScyllaDB                               | https://www.scylladb.com                         | ScyllaDB Software License Agreement 1.1, source-available (updated 2026-04-12). Release 6.2 was the last under AGPL-3.0                                   |
| `selene-db`      | selene-db                              | https://github.com/jscott3201/selene-db          | MIT OR Apache-2.0                                                                                                                                         |
| `starrocks`      | StarRocks                              | https://www.starrocks.io                         | Apache-2.0                                                                                                                                                |
| `tigerbeetle`    | TigerBeetle                            | https://tigerbeetle.com                          | Apache-2.0                                                                                                                                                |
| `tigergraph`     | TigerGraph                             | https://www.tigergraph.com                       | Proprietary                                                                                                                                               |
| `typedb`         | TypeDB (and TypeQL)                    | https://typedb.com                               | MPL-2.0                                                                                                                                                   |

## Publications

Research systems and methods known from their papers. A paper carries its publisher's copyright, not a
software licence; where the authors released code, its licence is given.

| Slug                      | Work                                                                                                                       | Link                                                                 | Code and licence                                                                   |
|---------------------------|----------------------------------------------------------------------------------------------------------------------------|----------------------------------------------------------------------|------------------------------------------------------------------------------------|
| `allen`                   | J. F. Allen, "Maintaining Knowledge about Temporal Intervals", CACM 26(11), 1983                                           | https://doi.org/10.1145/182.358434                                   | —                                                                                  |
| `bm25`                    | S. Robertson and H. Zaragoza, "The Probabilistic Relevance Framework: BM25 and Beyond", Foundations and Trends in IR, 2009 | https://doi.org/10.1561/1500000019                                   | —                                                                                  |
| `calvin`                  | A. Thomson et al., "Calvin: Fast Distributed Transactions for Partitioned Database Systems", SIGMOD 2012                  | https://cs.yale.edu/homes/thomson/publications/calvin-sigmod12.pdf   | https://github.com/yaledb/calvin, "Copyright (c) 2014 Yale University. All rights reserved."; no licence granted |
| `cascades`                | G. Graefe, "The Cascades Framework for Query Optimization", IEEE Data Engineering Bulletin 18(3), 1995                     | http://sites.computer.org/debull/95SEP-CD.pdf                        | —                                                                                  |
| `compartmentalized-paxos` | M. Whittaker et al., "Scaling Replicated State Machines with Compartmentalization", PVLDB 14, 2021                         | https://www.vldb.org/pvldb/vol14/p2203-whittaker.pdf                 | —                                                                                  |
| `hamsaz`                  | "Hamsaz: Replication Coordination Analysis and Synthesis", POPL 2019                                                       | https://mohsenlesani.github.io/companion/popl19/POPL19.pdf           | —                                                                                  |
| `homeostasis`             | "The Homeostasis Protocol: Avoiding Transaction Coordination Through Program Analysis", SIGMOD 2015                        | https://infoscience.epfl.ch/record/219208/files/final_camera_ready.pdf | —                                                                               |
| `invariant-confluence`    | P. Bailis et al., "Coordination Avoidance in Database Systems", PVLDB 8 (VLDB 2015)                                        | https://arxiv.org/pdf/1402.2237                                      | —                                                                                  |
| `ipa`                     | "IPA: Invariant-Preserving Applications for Weakly Consistent Replicated Databases", PVLDB 12 (VLDB 2019)                  | https://www.dpss.inesc-id.pt/~rodrigo/vldb-2019.pdf                  | —                                                                                  |
| `keep-calm`               | "Keep CALM and CRDT On", PVLDB 16 (VLDB 2023)                                                                              | https://www.vldb.org/pvldb/vol16/p856-power.pdf                      | —                                                                                  |
| `lucy`                    | M. Whittaker and J. M. Hellerstein, "Interactive Checks for Coordination Avoidance", PVLDB 12 (VLDB 2019)                  | https://mwhittaker.github.io/publications/iconfluence_VLDB19.pdf     | —                                                                                  |
| `lupine`                  | "A Linux in Unikernel Clothing" (Lupine Linux), EuroSys 2020                                                               | https://people.cs.vt.edu/djwillia/papers/eurosys20-lupine.pdf        | https://github.com/hckuo2/Lupine-Linux; no licence stated in the repository        |
| `query-rewrites`          | "Optimizing Distributed Protocols with Query Rewrites", SIGMOD 2024                                                        | https://arxiv.org/pdf/2404.01593                                     | —                                                                                  |
| `raft`                    | D. Ongaro and J. Ousterhout, "In Search of an Understandable Consensus Algorithm", USENIX ATC 2014                         | https://raft.github.io                                               | —                                                                                  |
| `ramp`                    | P. Bailis et al., "Scalable Atomic Visibility with RAMP Transactions", SIGMOD 2014                                         | http://www.bailis.org/papers/ramp-sigmod2014.pdf                     | —                                                                                  |
| `ramp-tao`                | "RAMP-TAO: Layering Atomic Transactions on Facebook's Online TAO Data Store", PVLDB 14, 2021                               | https://www.vldb.org/pvldb/vol14/p3014-cheng.pdf                     | —                                                                                  |
| `redblue`                 | C. Li et al., "Making Geo-Replicated Systems Fast as Possible, Consistent when Necessary" (Gemini), OSDI 2012              | https://web.eecs.umich.edu/~manosk/assets/papers/gemini.pdf          | —                                                                                  |
| `rrf`                     | G. V. Cormack, C. L. A. Clarke and S. Büttcher, "Reciprocal Rank Fusion Outperforms Condorcet and Individual Rank Learning Methods", SIGIR 2009 | https://doi.org/10.1145/1571941.1572114 | —                                                                    |
| `saltzer-schroeder`       | J. H. Saltzer and M. D. Schroeder, "The Protection of Information in Computer Systems", Proceedings of the IEEE 63(9), 1975 | https://www.cs.virginia.edu/~evans/cs551/saltzer/                    | —                                                                                  |
| `sans-io`                 | "Network protocols, sans I/O", a design pattern for protocol libraries                                                     | https://sans-io.readthedocs.io                                       | —                                                                                  |
| `vsr`                     | B. Liskov and J. Cowling, "Viewstamped Replication Revisited", MIT-CSAIL-TR-2012-021                                       | https://pmg.csail.mit.edu/papers/vr-revisited.pdf                    | —                                                                                  |
| `nist-rbac`               | ANSI INCITS 359-2004, Role Based Access Control (the NIST RBAC model)                                                      | https://csrc.nist.gov/projects/role-based-access-control             | Standard, © ANSI                                                                   |

## Standards

The query languages, data formats and protocols UltraGraphDB implements.

| Slug           | Standard                                                                                   | Website                                         | Terms                                                                     |
|----------------|--------------------------------------------------------------------------------------------|-------------------------------------------------|---------------------------------------------------------------------------|
| `gql`          | ISO/IEC 39075:2024, GQL                                                                    | https://www.iso.org/standard/76120.html         | ISO copyright; sold by ISO                                                |
| `opencypher`   | openCypher and its Technology Compatibility Kit                                            | https://opencypher.org                          | Apache-2.0                                                                |
| `bolt`         | Bolt protocol                                                                              | https://neo4j.com/docs/bolt/current/            | Published by Neo4j                                                        |
| `sparql`       | SPARQL 1.1 and 1.2, SPARQL Update, SPARQL Protocol, SPARQL Service Description             | https://www.w3.org/TR/sparql11-query/           | W3C Document License                                                      |
| `rdf`          | RDF 1.2, RDFS, XSD datatypes, VoID                                                         | https://www.w3.org/TR/rdf12-concepts/           | W3C Document License                                                      |
| `owl`          | OWL 2                                                                                      | https://www.w3.org/TR/owl2-overview/            | W3C Document License                                                      |
| `shacl`        | SHACL                                                                                      | https://www.w3.org/TR/shacl/                    | W3C Document License                                                      |
| `geosparql`    | GeoSPARQL 1.1                                                                              | https://www.ogc.org/standards/geosparql/        | OGC Document License                                                      |
| `geojson`      | GeoJSON, RFC 7946                                                                          | https://www.rfc-editor.org/rfc/rfc7946          | IETF Trust Legal Provisions (BCP 78)                                      |
| `qudt`         | QUDT quantities, units and dimensions                                                      | https://qudt.org                                | CC-BY-4.0                                                                 |
| `ucum`         | Unified Code for Units of Measure                                                          | https://ucum.org                                | UCUM Copyright Notice and License (Regenstrief Institute)                 |
| `xxh3`         | XXH3-64, as the xxHash specification defines it                                            | https://github.com/Cyan4973/xxHash              | Library: BSD-2-Clause                                                     |
| `ietf-auth`    | SASL (RFC 4422), SCRAM-SHA-256 (RFC 5802, 7677), `tls-exporter` (RFC 9266), `OAUTHBEARER` (RFC 7628), JWT (RFC 7519, 8725), TLS 1.3 (RFC 8446) | https://www.rfc-editor.org | IETF Trust Legal Provisions (BCP 78)                         |
| `oidc`         | OpenID Connect Core 1.0 and Discovery 1.0                                                  | https://openid.net/developers/specs/            | OpenID Foundation                                                         |
| `mcp`          | Model Context Protocol                                                                     | https://modelcontextprotocol.io                 | Apache-2.0 for specification and code, moving from MIT; documentation CC-BY-4.0 |
