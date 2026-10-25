# UltraGraphDB Roadmap: Dual Query Engine and Tile Storage

Oct 24, 2026 · @Marvin Hansen

## Summary

UltraGraphDB replaces Dgraph's DQL with standard query languages in two stages. Stage 1 ships conformant SPARQL and GQL engines over the existing Dgraph key-value storage, behind one typed binary protocol. Stage 2 adds a tile-based storage engine for distributed analytics and the typed UltraQL language, and makes both standard engines faster without giving up the conformance won in Stage 1.

|  | Stage 1: Conformance | Stage 2: Scale |
| --- | --- | --- |
| Primary goal | Pass the conformance suites; reasonable performance on regular queries | Fast distributed analytics and deep multi-hop queries; larger-than-memory and multi-node graphs |
| Storage | Existing Dgraph KV engine (predicate-sharded posting lists), extended with statement IDs | New tile storage (2D-partitioned CSR/CSC, GSL addressing); KV engine kept as the write layer |
| Languages | SPARQL 1.1 (1.2 later), ISO GQL, openCypher via Bolt | Same, plus UltraQL |
| Protocol | One binary TCP protocol with typed query dispatch; Bolt adapter for openCypher | Unchanged; UltraQL added as a language tag |
| Exit gate | Conformance targets met; baseline benchmarks recorded | Same conformance results; benchmark gains on the same suites |

**Goals**

- Standards first: users can bring existing SPARQL, GQL and Cypher skills and queries.
- One store, two data models: RDF and labeled property graphs as views over one canonical statement model.
- Conformance is a release gate, not an aspiration, in both stages.
- Zero-copy binary transport for low serialization overhead.

**Non-goals for Stage 1**

- DQL compatibility. It may return later as a migration front end if existing Dgraph users need one.
- Distributed graph analytics and the new storage engine.
- UltraQL. Stage 1 only designs the schema catalog so that UltraQL can be added without breaking changes.

## Stage 1 architecture

Stage 1 runs two separate query engines, one per standard, over a shared physical kernel and the existing Dgraph storage. Experience shows one optimizer cannot serve two standards that diverge on edge cases, so semantics stay per engine while storage and execution primitives are shared.

### Binary protocol with typed dispatch

There is one protocol and one endpoint: a binary protocol over a standard TCP connection. Every request declares its language in the envelope, and the server dispatches it to the matching engine.

- **Envelope:** a custom byte-level format in the style of Apache Iggy for requests, prepared statements, transactions and errors. Headers are fixed-size and little-endian; payloads use separate index and data vectors, so messages are read as views over the raw bytes, without a serialization library or FFI. The language is an extensible enum: `Sparql`, `Gql`, with `UltraQl` reserved for Stage 2.
- **Results:** the same byte-level format, laid out by column: an offsets vector and a data vector per column, plus a validity bitmap. The buffer layout follows Arrow's, so clients that want Arrow can wrap the buffers without copying, but no Arrow library is required.
- **Required from day one:** a version handshake with feature negotiation; TLS and authentication in the handshake; multiplexed queries per connection; query cancellation; credit-based flow control for streamed batches; prepared statements with server-side plan caching.
- **Untrusted input:** every frame is bounds-checked against its declared length, and every offset in its index vectors is validated once, before any view is handed out.
- **Evolution rule:** fields are only added, never repurposed, and every frame carries a version. The format is specified byte by byte, with a shared suite of valid and malformed test frames that every client library must pass.

### openCypher via Bolt

A Bolt protocol adapter accepts openCypher from the Neo4j driver ecosystem: drivers in every major language, LangChain and LlamaIndex graph connectors, and visualization tools. The adapter is a thin, stateless gateway that translates Bolt into the native protocol and routes openCypher to the GQL engine. The GQL engine's parser accepts the Cypher dialect where it overlaps with GQL. Cypher-only constructs are either mapped to GQL equivalents or rejected with a clear error.

A SPARQL Protocol (HTTP) gateway built the same way is optional. It would let existing SPARQL tools connect without a native driver.

### Two engines

| Layer | SPARQL engine | GQL engine |
| --- | --- | --- |
| Parser and algebra | `spargebra` (Oxigraph; MIT/Apache-2.0) | Forked from the selene-db parser and analyzer (MIT/Apache-2.0), with a GQL Flagger for implementation-defined extensions |
| Semantic analysis | Unbound variables, FILTER errors, set/bag rules per SPARQL | Typed values, nulls with three-valued logic, match and path modes |
| Logical plan and rewrites | SPARQL algebra rules | GQL-specific rules |
| Expression evaluation | RDF terms, effective boolean value, error propagation | Typed GQL values, null propagation |
| Results | RDF terms as columns (lexical form, datatype, language tag) | GQL values as columns |

### Shared kernel

The kernel holds everything that carries no language semantics:

- Storage access through a declarative access-pattern interface (bound, unbound and projected positions over the statement relation).
- Physical operators: sorted posting-list intersection, worst-case-optimal (leapfrog) joins, hash and index nested-loop joins, sort, aggregation primitives, and one path-search automaton that takes a path mode as a parameter.
- Statistics: per-predicate triple counts, distinct subject and object counts, degree histograms, and characteristic sets.
- Transactions and snapshots: both engines read the same snapshot and commit through one write path.
- The catalog and the RDF/LPG mapping contract.

The planners call kernel operators with explicit flags (null handling, match isomorphism mode, path mode), so semantics are decided in each engine and never inside the kernel.

### Storage changes on the Dgraph KV engine

- Extend keys to include the graph: (graph, predicate, subject) maps to a sorted list of (object, optional statement ID).
- Allocate a statement ID only for edges with facets and for parallel S-P-O edges. Plain triples, the vast majority, keep the existing fast path.
- Store facets as properties of the statement ID, which is the RDF 1.2 reifier.
- Make reverse indexes mandatory for edge predicates, so worst-case-optimal joins can intersect in either direction.
- Put an entity type prefix in the high bits of UIDs, assigned in preorder of the type hierarchy. Stage 1 only reserves the bits; Stage 2 relies on them.

The diagram below shows how these parts connect.

&#91;embedded content: Stage 1 architecture · two engines over one kernel\]

Semantics live only in the two engines; everything below them is shared and language-neutral.

## Stage 1 data model: the mapping contract

RDF and property graphs are two views over one canonical model: quads with an optional statement ID, (graph, subject, predicate, object, SID). Because the two engines no longer share an optimizer, this contract is the only coupling between them. It is written as a versioned specification before either engine is built, and the cross-language tests check it.

| Concept | Canonical store | RDF / SPARQL view | LPG / GQL view |
| --- | --- | --- | --- |
| Node | UID with a type prefix | IRI (local nodes get `base:uid` IRIs; blank nodes are skolemized and flagged) | Node with element identity |
| Label | `rdf:type` statement | `rdf:type` triple | Label |
| Property | Predicate with a literal object | Triple with a literal | Property |
| Edge | Predicate with a UID object | Triple | Edge |
| Edge identity | Statement ID, allocated only when needed | Reifier (RDF 1.2) | Edge identity |
| Parallel edges | Several SIDs for one S-P-O | Projected as one distinct triple | Separate edges |
| Edge property (Dgraph facet) | Property of the SID | Annotation on the reifier | Edge property |
| Multi-valued property | Several objects for one S-P | Several triples | Scalar when single-valued, LIST when multi-valued (declared per predicate) |
| Named graph | Graph component of the key | Named graph | Graph in the catalog |

**Datatypes.** Literals are stored complete: lexical form, datatype, language tag and direction. A documented XSD-to-GQL type table defines the GQL view, for example `xsd:integer` to INT64 with an explicit overflow rule, `xsd:dateTime` to ZONED DATETIME, and `rdf:langString` to STRING with the language tag available through a function. Ill-typed literals stay intact for SPARQL and appear in GQL as raw strings or nulls, according to a stated rule.

**Annotations without an asserted triple.** RDF 1.2 allows annotating a triple term that is not asserted. Such annotations have no edge in the GQL view and are hidden from it.

**One write path.** SPARQL Update and GQL mutations compile to the same storage changeset format and commit through the same transaction layer. Separate write paths would produce inconsistent edge identity.

**Schema catalog.** The catalog is designed for the most expressive language from the start: entity types with single inheritance and abstract types, n-ary relations with named roles, first-class attributes, and declared cardinalities. SHACL (RDF side) and GQL graph types (LPG side) are two surface syntaxes over it, and UltraQL becomes the third in Stage 2.

## Stage 1 conformance and testing

Conformance is checked in three layers: specification suites, differential testing against reference implementations, and metamorphic testing within and across languages. All three run in CI and gate every release.

| Layer | SPARQL engine | GQL engine and openCypher |
| --- | --- | --- |
| Specification suites | W3C SPARQL 1.1 suites (query, update, protocol, results formats); SPARQL 1.2 suites when 1.2 is enabled | No official ISO suite exists. Use the openCypher TCK (2024.x releases, which track GQL), plus our own GQL scenarios in the same Gherkin format, mapped to ISO feature IDs |
| Reference oracle | Oxigraph's `spareval`, running on UltraGraphDB storage through its `QueryableDataset` trait | selene-db, loaded with the same LPG view |
| Metamorphic testing | Ternary logic partitioning (GDBMeter style) and graph-aware data mutations (Gamera style) | Same, adapted to GQL syntax |

**Reference oracle on our own storage.** Implementing spareval's `QueryableDataset` trait over the posting lists gives a reference SPARQL evaluator that reads exactly the same data as our engine. A difference in results then points to our planner or executor, not to data loading. It also provides a working, conformant SPARQL endpoint before our own engine is finished.

**Triage diffs against the specification, not the oracle.** Differential testing of graph databases has a known false-positive problem when implementations make different but legal choices. Each oracle is pinned to a feature subset where it is known to be conformant, and every difference is judged against the specification text.

**Cross-language tests on the safe overlap.** A generator produces one abstract pattern and renders it in both SPARQL and GQL. On the fragment where the two semantics agree (basic patterns, no OPTIONAL, no paths, homomorphic matching), results must be equal under the mapping contract. These tests check the mapping and the shared write path, not optimizer equivalence.

**GQL feature register.** The GQL engine keeps a machine-readable register of claimed optional features, in the style of selene-db. The Flagger rejects unclaimed constructs at parse time. Release notes publish the register, including known gaps.

**Performance baselines.** Stage 1 records baselines that Stage 2 must beat. Candidate suites: the LDBC Social Network Benchmark interactive workload for GQL, a SPARQL benchmark such as BSBM or the Wikidata query logs, and a micro-benchmark set for supernode reverse lookups.

## Stage 1 milestones and exit criteria

SPARQL comes first because Dgraph's data is already RDF-shaped and the SPARQL oracles are the strongest. GQL reads follow, and GQL mutations come last because edge-identity updates are the hardest part. Dates are not set yet.

1. **M1: Foundations.** Mapping contract v1 written; catalog schema defined; storage key extensions (graph component, statement IDs, mandatory reverse indexes, reserved type bits) implemented; binary protocol v1 with handshake, multiplexing and cancellation.
   - Exit: the protocol carries `Sparql` and `Gql` requests end to end; storage round-trips quads with statement IDs.
2. **M2: SPARQL through the reference evaluator.** spareval runs on UltraGraphDB storage through `QueryableDataset`. The W3C suite harness is in CI.
   - Exit: W3C SPARQL 1.1 query and update suites pass through the reference path.
3. **M3: Native SPARQL engine.** `spargebra` lowers to our logical plan; cost-based binary-join planner using posting-list statistics; worst-case-optimal joins for cyclic patterns; differential testing against spareval in CI.
   - Exit: the native engine passes the same W3C suites; no unexplained differences from spareval on generated queries; SPARQL baselines recorded.
4. **M4: GQL reads.** Parser and analyzer forked from selene-db; the Flagger and feature register; GQL planner on the shared kernel; path operator with GQL path modes.
   - Exit: GQL scenario suite and the openCypher TCK read scenarios pass at an agreed rate; differential tests against selene-db pass; cross-language tests on the safe overlap pass.
5. **M5: GQL mutations and Bolt.** GQL writes through the shared changeset path; Bolt adapter with openCypher routing.
   - Exit: write scenarios in the TCK pass at an agreed rate; Neo4j drivers in at least two languages connect and run the TCK through Bolt; LDBC interactive baseline recorded.

**Stage 1 exit gate:** published conformance numbers for SPARQL 1.1, openCypher TCK and the GQL feature register; recorded performance baselines; mapping contract at a stable version.

## Stage 2 architecture: tile storage

Stage 2 stores graph topology as a hierarchically tiled sparse matrix: 2D-partitioned CSR and CSC blocks addressed by a GSL index. The existing KV engine stays as the write-optimized layer, so Stage 1's transaction machinery carries over.

### Matrix layout

- **Dense, type-clustered IDs.** Each entity has a dense integer ID with its type in the high bits, assigned in preorder of the type hierarchy. A subtype is a contiguous ID range, and a supertype is one larger range covering its subtypes. The type prefix makes the ID space sparse, so tiles use hypersparse formats (DCSR and DCSC) that skip empty rows.
- **One matrix per relation or role, stored both ways.** CSR serves forward traversal and CSC serves reverse traversal. N-ary relations are stored as one incidence matrix per role (relation instances × role players).
- **Adjacency ordered within each list** by neighbor type, then by key edge attributes. Type filters and common edge filters become range reads instead of scans.
- **Attributes** are per-type columnar arrays indexed by dense ID, which match the protocol's columnar layout and go out without conversion.
- **Tile encoding.** Plain DCSR/DCSC by default. k²-trees are an option for tiles where compression and reverse lookups matter most.

QTT (quantized tensor train) was considered and rejected as the storage format. Real graphs are irregular, so exact TT ranks grow with the number of distinct non-empty blocks and storage can exceed CSR; exact edge updates also do not fit TT rounding. The hierarchical addressing idea behind it is kept through GSL.

### GSL addressing

| Level | Meaning | Role |
| --- | --- | --- |
| G (group) | Availability group | Replication and failover, one consensus group per G |
| S (shard) | Position in the 2D tile grid | A row range × column range of the matrix assigned to the group |
| L (local) | Tile on a node | Unit of paging, caching, versioning and compaction |

With 2D partitioning, each traversal step communicates along one process row and one process column, about √p peers instead of all p. A supernode's row is split across column tiles and machines automatically.

### Write path and snapshots

- Writes commit to the KV layer as in Stage 1, which acts as the delta store.
- Background compaction merges deltas into new immutable tile versions, each stamped with an epoch.
- A snapshot is a set of tile versions plus the KV deltas newer than them. Reads merge the base tile with recent deltas.
- Prior art for dynamic CSR to evaluate: PCSR, Terrace, Aspen, Sortledton.

### Larger than memory and larger than one node

- **Larger than memory:** tiles are paged. Hot tiles stay resident, cold tiles stream from NVMe, and traversals skip tiles that contain no active vertices (as in GridGraph's two-level grid).
- **Larger than one node:** tiles are distributed over the 2D grid as in CombBLAS and distributed GraphBLAS. Groups replicate their tiles for availability.

### Traversal execution

Each hop is a sparse matrix × sparse frontier vector product with a visited mask. The kernel adds:

- Filters pushed into the expansion as range reads, so cost scales with the edges that survive the filter.
- Frontier deduplication, which bounds each hop by the number of vertices.
- Push/pull switching when the frontier grows large.
- Search from both ends when both endpoints are known, reducing cost from b^n to about 2·b^(n/2).
- Semiring variants for shortest paths and weighted aggregation.

The diagram below shows the Stage 2 layout.

&#91;embedded content: Stage 2 storage · KV write layer, tile read store, GSL addressing\]

The KV layer keeps Stage 1's transactions; tiles are a read-optimized form rebuilt from it by compaction.

## Stage 2: UltraQL and the standard engines

UltraQL is a third engine on the shared kernel, and the tile storage speeds up all three. Conformance from Stage 1 is preserved by keeping each engine's semantics unchanged and swapping only physical operators underneath.

### UltraQL

UltraQL is a typed successor to DQL. It takes shape-preserving tree results from DQL, the schema model from TypeQL (n-ary relations with roles, first-class attributes, abstract types and inheritance), and type inference from Rust.

```
Aircraft {
  tail_number
  all_parts {
    serial
    type
    on Avionics { firmware }
  }
}
```

- **The query is the shape of the result.** Filters sit on types, relations read as fields, and nesting implies grouping.
- **The schema carries the semantics:** cardinality, subtyping, role names, acyclicity and derived relations such as transitive closures are declared once.
- **No implicit nulls.** A field that does not apply to a type is absent; an unknown value is an `Option`.
- **Explicit `match` for cyclic patterns**, where variables are actually needed.
- **Developer experience:** compiler diagnostics in the style of Rust's, a language server driven by the schema, and a client macro that checks queries at build time and generates typed result structs.

UltraQL maps directly onto tile storage. Navigation fields are matrix transposes and compositions. Subtype filters are ID ranges, which means skipping tiles. Transitive closures are iterated expansions, and results go out as columnar trees with no conversion. Some ontology features may first ship as flagged GQL extensions to test demand.

### Benefits for SPARQL and GQL

| Area | Stage 1 (KV) | Stage 2 (tiles) |
| --- | --- | --- |
| Point lookups and one hop | Key fetch and decode per list | Offset read and contiguous scan; merge with recent deltas |
| Multi-hop and deep queries | One key fetch per vertex per hop | Batched frontier expansion with pushed-down filters and deduplication |
| Supernodes | One large posting list per hub predicate | Hub rows split across tiles; type filters skip tiles; pull-direction traversal |
| Joins | Posting-list intersection | Same algorithms over cache-friendly tiles |
| Analytics | Not supported | Matrix-vector algorithms (PageRank, BFS, components) across the cluster |

### Preserving conformance

- Planners and expression evaluators are not changed by the storage migration. Only access paths and physical operators change.
- Every conformance suite from Stage 1 runs against both storage back ends during the migration. Results must be identical.
- A storage-differential test runs the same generated queries on the KV path and the tile path and compares the results.
- The tile path can be enabled per graph, with the KV path as a fallback until the tile path has passed the full gate.

## Stage 2 milestones and exit criteria

Tile storage ships in steps from read-only analytics to the primary read path, and UltraQL starts once the tile path is stable. Dates are not set yet.

1. **M6: Read-only tiles for analytics.** Compaction builds immutable tiles from the KV store; matrix-vector traversal kernels; single-node analytics (BFS, PageRank, connected components).
   - Exit: analytics results match a reference implementation; tiles rebuild deterministically from the KV store.
2. **M7: Tiles as a read path for SPARQL and GQL.** Access patterns served from tiles plus KV deltas; storage-differential testing.
   - Exit: all Stage 1 conformance suites pass identically on both back ends; Stage 1 baselines improved on multi-hop and supernode benchmarks.
3. **M8: Larger than memory and distributed.** Tile paging from NVMe; 2D distribution over GSL groups; replication per group.
   - Exit: a graph larger than one node's memory runs the benchmark suites; a group failure is survived without data loss.
4. **M9: UltraQL read language.** Schema surface syntax over the catalog; parser, type checker with diagnostics, planner on the kernel; columnar tree results; language server.
   - Exit: UltraQL test suite passes; results agree with equivalent GQL and SPARQL queries on the shared fragment.
5. **M10: UltraQL writes and client tooling.** Atomic n-ary relation inserts; schema evolution; build-time checked client macro for Rust.
   - Exit: write tests pass; schema migration tests pass; one external user pilots UltraQL.

**Stage 2 exit gate:** Stage 1 conformance numbers unchanged on the tile path; published benchmark gains for deep and multi-hop queries; distributed deployment documented.

## Risks and open questions

The largest risks are semantic drift between the two views of the data and the engineering size of a second storage engine.

| Risk | Stage | Mitigation |
| --- | --- | --- |
| Mapping contract gaps surface late (edge identity, multi-valued properties, datatypes) | 1 | Write the contract first; cross-language tests on the safe overlap from M4 |
| No official GQL conformance suite | 1 | openCypher TCK plus our own scenarios mapped to ISO feature IDs; publish the feature register |
| Oracle false positives in differential testing | 1 | Pin oracles to known-conformant subsets; judge differences against the specification |
| Three planners to maintain | 1–2 | Keep the kernel free of language semantics; share operators and statistics |
| Custom protocol limits ecosystem reach | 1 | Bolt adapter for openCypher; optional SPARQL Protocol gateway |
| Compaction cost and hot tiles under heavy writes | 2 | KV layer as delta store; evaluate PCSR, Terrace, Aspen and Sortledton before choosing |
| Type-prefixed ID space (reserved ranges, hierarchy changes, entity type changes) | 2 | Reserve bits in Stage 1; define a migration path for type changes |
| Multiple inheritance breaks contiguous type ranges | 2 | Single inheritance for entities; small unions of ranges otherwise |

**Open questions**

- [ ] Which RDF 1.2 status do we target: Full or Basic conformance? (RDF 1.2 is still a Candidate Recommendation.)
- [ ] Result layout: keep Arrow-compatible column buffers, or specialize the layout further for graph results?
- [ ] Which Cypher-only constructs does the Bolt path support, and which are rejected?
- [ ] Does DQL return as a migration front end for existing Dgraph users?
- [ ] UltraQL mutation syntax, query composition, and where functions or derived relations live.
- [ ] Benchmark suites and the numeric targets for each exit gate.
