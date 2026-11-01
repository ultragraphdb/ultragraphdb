# Agentic Context Projection

Nov 1, 2026 · @Marvin Hansen

UltraGraphDB can serve as the single source of truth for agentic knowledge systems: it holds the **ontology** (what exists and how it relates), the **knowledge** (typed facts) and the **links** to relevant information (documents and passages, with provenance). **Context projection** is the operation that turns this into what an agent needs: one query that anchors on the task, scopes through the graph and access rights, ranks passages by meaning and keywords, and returns a cited, permission-scoped, reproducible context pack. This replaces the usual assembly of a relational database, a knowledge graph and a vector store joined by glue code. Companion documents: [UltraGraph Schema and UQL](https://claude.ai/artifact/LtXDHoVcin1aGFJWDxZDbL) and [UltraGraphDB Distribution Architecture](https://claude.ai/artifact/W7YBsh2yPKCLjehZiEsFZD).

## The problem

Agent-facing knowledge bases need three steps: traverse the knowledge graph to find what is relevant to the current task, narrow that to the applicable documents, and rank their passages by semantic and keyword relevance. In a typical enterprise these steps span three systems: relational data in one database, a knowledge graph in another, embeddings in a dedicated vector store. That brings recurring problems:

- **Glue code** that moves ID lists between systems and re-implements joins in the application.
- **Inconsistency:** each system is current to a different moment, so a deleted document can still be retrieved from the vector store, or a new relation is invisible to the vector search.
- **Leakage:** access rights live in one system and embeddings in another, so permission filtering happens after ranking, if at all.
- **Recall loss:** when the graph narrows the candidates after the vector search, most of the top results are discarded, and the useful ones were never fetched.
- **Ontology blindness:** the agent does not know what types and relations exist, so it guesses query terms and gets empty results without knowing why.
- **Context bloat:** results arrive as flat rows or loose chunks, with repeated data and no citations, consuming tokens without adding information.
- **No audit trail:** nobody can reconstruct what the agent saw when it made a decision.

## The idea: projection, not integration

A database view projects a stored model into the shape one application needs. Context projection does the same for an agent: it projects the stored ontology, knowledge and links into the shape one task needs, at one consistent point in time, filtered by what the requesting user may see.

&#91;embedded content: Context projection · five stages in one plan\]

| Stage | What happens | What UltraGraphDB contributes |
| --- | --- | --- |
| Anchor | Find the entities the task is about | Keys, full-text and vector search over entities |
| Scope | Narrow to relevant, permitted documents | Typed traversal, relation subtypes, access-control relations |
| Rank | Score passages within the scope | Vector similarity and BM25, fused, inside the graph-defined scope |
| Expand | Attach documents, sources, related facts | Navigation, provenance on reifiers, IRIs for citations |
| Shape | Bound and structure the result | Typed nested results, per-branch limits |

## Ontology for agents

An agent can only ask good questions of a knowledge base whose structure it understands. UltraGraphDB exposes its ontology directly:

- **Introspection.** The schema is a read-only graph the agent can query, including the documentation written in `///` comments:

```
schema.types(name starts_with "Doc") {
  name  doc
  attributes  { name  type  optional  doc }
  navigations { name  relation { name }  cardinality }
}
```

- **Errors that teach.** A query that names a non-existent field, or asks a subtype field of a supertype, fails at compile time with a suggested fix, instead of silently returning nothing.
- **Typed results.** The agent knows before execution whether a field is a value, optional, or a list.
- **Standard views.** The same ontology is available as RDF vocabulary and SHACL for SPARQL clients and as a GQL graph type, so existing ontology tools work against it.

## Modelling knowledge and documents

```
enum DocKind { manual, service_bulletin, incident_report }

entity Principal @key(name) { name: String }

/// A source document; chunks hold its passages.
entity Document @key(doc_id) {
  doc_id: String
  title:  Text @index(fulltext)
  kind:   DocKind
  issued: Date
}

/// A passage of a document, with its embedding.
entity Chunk {
  text:      Text @index(fulltext)
  embedding: Vector<BF16, 768> @index(ann, metric: cosine)
  position:  UInt32
}

relation HasChunk @colocate(document) {
  document: Document -> chunks*
  chunk:    Chunk    -> document
}
relation About { document: Document -> subjects*  subject: Asset -> documents* }
relation ReadableBy @access {
  document:  Document  -> readers*
  principal: Principal -> readable*
}
```

- Documents link into the domain graph through `About`, so domain entities (aircraft, engines, suppliers) lead to their documents and back.
- Chunks are colocated with their document, so a document's passages are scored in one group.
- `@access` marks access-control relations, which changes how queries over them are read (see consistency below).

## A projection in one query

*An agent asks about vibration limits for the engines of one aircraft, on behalf of one user.*

```
Aircraft(tail_number = $tail) {
  tail_number
  engines {
    serial
    evidence: documents(kind in [manual, service_bulletin],
                        readers contains Principal(name = $user))
              .chunks ~ hybrid(nearest(embedding, $q_vec), search(text, $q_text), k: 8) {
      text
      score
      document { title  issued  iri }
    }
  }
}
```

1. **Anchor:** the aircraft is found by key.
2. **Scope:** the graph narrows the search space to its engines, the documents about them, only manuals and service bulletins, and only what the user may read.
3. **Rank:** vector similarity and BM25 relevance are fused over the chunks of those documents only.
4. **Expand and shape:** each passage comes back with its document as a nested tree, ready to be placed into the agent's context with citations.

## Projection functions

Composing projections is a design task; running them is not. Schema functions let a team define curated projections once, so agents call a named, reviewed projection instead of writing retrieval logic themselves:

```
/// Maintenance evidence for an aircraft's engines, scoped to what the user may read.
fn Aircraft.engine_evidence(q_vec: Vector<BF16, 768>, q_text: Text, user: String) -> Chunk* =
  engines.documents(kind in [manual, service_bulletin],
                    readers contains Principal(name = user))
         .chunks ~ hybrid(nearest(embedding, q_vec), search(text, q_text), k: 8)
```

```
Aircraft(tail_number = $tail) {
  engine_evidence($q_vec, $q_text, $user) { text  score  document { title  iri } }
}
```

Projection functions are versioned with the schema, documented through `///` comments, discoverable through introspection, and inlined into the calling plan, so they cost nothing at runtime.

## How scoped vector search executes

The planner estimates the size of the candidate set from schema cardinalities and statistics, and chooses a strategy per query:

| Candidate set from the graph | Strategy | Recall |
| --- | --- | --- |
| Small (up to a few thousand chunks) | Exact scoring: BF16 dot products over the candidates | Exact |
| Medium | Filtered approximate search: the traversal produces an ID bitmap, and the index search skips vectors outside it | High |
| Large, unselective scope | Approximate search first, then graph filtering with over-fetch | Approximate, tuned by over-fetch |

The enabling detail is the dense, type-prefixed ID scheme: any graph-derived candidate set (a traversal, an access-control check, a relation subtype) becomes a compressed bitmap that the vector index consults during search. Filters therefore do not need to be declared when the index is created. Filtered approximate search degrades when filters are very selective, which is exactly the range where the planner switches to exact scoring.

Neo4j, the closest comparison, now offers in-index filtering, pre-filtering and post-filtering, but its in-index filtering requires the filtered properties to be identified at index creation; richer graph logic has to run before the vector search (exact but costly for large candidate sets) or after it (with recall loss unless results are over-fetched). UltraGraphDB's difference is that arbitrary graph predicates become in-index filters, and the planner, not the query author, chooses the strategy.

## Fusion and ranking

- `hybrid(...)` fuses its inputs by reciprocal rank fusion by default; `weights: [0.7, 0.3]` switches to weighted score fusion.
- `k` applies per branch, so each engine contributes its own top passages; `limit` on outer levels bounds the total.
- Re-ranking with an external cross-encoder stays outside the database: the projection returns candidates and scores for it.

## Consistency and access control

- **One snapshot.** Full-text and vector indexes are derived data, maintained through change streams and stamped with epochs, so a snapshot read sees graph, text and vectors at the same point. A deleted document cannot surface through a stale vector index.
- **Permissions before ranking.** Access-control relations scope the candidate set before anything is scored, so passages the user may not read never reach the ranking, the scores or the agent.
- **`@access` forces snapshot reads.** A query that filters on an `@access` relation always runs at a snapshot epoch, never monotonic-fast, so a revoked permission takes effect immediately.

## Context shaping

- **No repetition.** The nested result states each entity once, however many passages hang below it.
- **Bounded size.** Per-branch `k` and `limit` control how much each part of the context contributes.
- **Citations built in.** Every passage carries its document's IRI; provenance recorded on reifiers (source, extraction model, confidence) can be projected alongside it.
- **Columnar transport.** Results travel as Arrow-compatible buffers, so Python and Rust clients read them without copying before rendering them into a prompt.

## Reproducibility and audit

Every response carries the epoch it was read at. `as of epoch e` replays a projection exactly as the agent saw it, including the access rights in force at that moment. An audit of an agent decision can therefore reconstruct its evidence, not just its output.

## Agent memory and write-back

Agents also produce knowledge: extracted facts, observations, summaries. Writing these back keeps the knowledge base the single source of truth:

- **Append-only memory.** Agent observations modelled as `@append_only` relations are coordination-free to write and, because they are never retracted, safe to read monotonic-fast: an agent's own memory is fast to recall and never contains a false fact.
- **Provenance on every derived fact.** RDF 1.2 reifiers record source, model, confidence and valid time, so derived facts stay distinguishable from curated ones.
- **Derived facts through pipelines.** A pipeline can turn retrieval results into new relations in one atomic statement, for example linking incident reports to the components they mention.

## Interfaces for agents

| Interface | Use |
| --- | --- |
| MCP server | Exposes introspection, projection functions and free-form UQL as agent tools |
| Native protocol, Python and Rust clients | Typed, zero-copy access for agent frameworks and services |
| Bolt | Compatibility with existing Neo4j-based agent tooling |
| SPARQL endpoint | Ontology tools and linked-data clients |

## Working with existing systems

There are two integration paths: ingestion in the first version, and remote queries with index replication in version 2.

### Ingestion (first version)

Where data volumes allow, existing systems connect by ingestion: relational systems through change data capture into typed entities, existing property graphs through Bolt and Cypher export, and documents chunked and embedded by external pipelines before ingestion, then written as chunks with vector attributes. Chunking and embedding stay outside the database, so organizations keep their own models and pre-processing. A projection then covers everything, at one snapshot.

### Remote queries and index replication (planned for version 2)

Many enterprises cannot ingest their existing data: a multi-petabyte data warehouse is not going to be copied into another database. Version 2 therefore adds **remote sources**, modelled on PostgreSQL's foreign data wrappers (`postgres_fdw`, and the `oracle_fdw` extension), so existing systems integrate without modification, migration or complex integration work.

The principle is to replicate the index, not the data. UltraGraphDB keeps only what scoping and ranking need (keys, join attributes, embeddings and full-text indexes over selected columns) and fetches full rows from the remote system on demand.

```
source warehouse = remote(oracle, "dwh.example.internal:1521/DWH")

/// Work orders stay in the warehouse; only keys and indexes are replicated.
external entity WorkOrder from warehouse."MAINT"."WORK_ORDERS" @key(order_no) {
  order_no:  String          = "ORDER_NO"
  serial:    Serial          = "ASSET_SERIAL"  @replicate
  opened:    Date            = "OPENED_AT"
  summary:   Text            = "SUMMARY"       @replicate(index: fulltext)
  embedding: Vector<BF16, 768> @replicate(index: ann)   // from the external embedding pipeline
}

relation Concerns @external(join: WorkOrder.serial = Asset.serial) {
  order: WorkOrder -> asset
  asset: Asset     -> work_orders*
}
```

The remote table now appears in the ontology like any other type. A projection can traverse from an aircraft to its engines, to their work orders in the warehouse, rank those by meaning, and fetch only the top results' full rows.

| Part | Where it lives |
| --- | --- |
| Full rows | In the remote system, fetched on demand |
| Keys and join attributes | Replicated index in UltraGraphDB, linking remote rows into the graph |
| Full-text and vector indexes over selected columns | Replicated in UltraGraphDB, so scoping and ranking run locally |
| Ontology mapping | The UGS schema (`external entity`, `@external` relations) |

**Execution.** As with foreign data wrappers, the planner pushes filters, projected columns and limits down to the remote system. Scoping and ranking run locally on the replicated index, so the remote system answers only narrow key lookups for the final results rather than scans.

**Freshness and consistency.** The replicated index is kept current by change data capture where the remote system offers it, otherwise by incremental refresh. Remote rows reflect the remote system's own current state, not UltraGraphDB's snapshot epoch, so results from external types carry their source and the index's freshness. Point-in-time replay (`as of`) covers external data only where the remote system supports historical reads, such as Oracle's flashback queries.

**Access control.** The remote system's own permissions remain authoritative for its data; local `@access` relations apply in addition.

**Scope.** External types are read-only, never monotonic-fast (they sit outside the epoch model), and require no changes to the remote system beyond read access and, optionally, a change feed.

## Positioning

UltraGraphDB is not a general-purpose graph database. It is purpose-built for DeepBrain: massive, multi-domain, deeply interconnected knowledge whose ontology is executable, so that knowledge becomes actionable for counterfactual reasoning, agentic systems, or both. It does not try to solve the general case; it tries to solve this one case very well.

Several databases combine graph, text and vector search in one query language, so that alone is not the differentiator. The differentiator is that **the ontology is executable**: one schema decides

1. the types every query is checked against;
2. the commit path of every write, paying for coordination only where a declared invariant requires it;
3. which reads can be served fast, and with what guarantee;
4. where data lives, so constraints are checked in one group;
5. the standard GQL, RDF and SHACL views.

Declaring a constraint is therefore also declaring its cost, and the coordination report shows that cost when the schema compiles.

For agentic and counterfactual workloads this yields:

- **Strict consistency at the cost the knowledge requires.** Exact snapshot reads across groups, without coordinating writes that no invariant puts at risk.
- **Guarantees stated in the language.** Append-only knowledge read monotonic-fast never yields a false fact; permissions apply before ranking; revocation takes effect immediately.
- **One timeline for everything.** Graph, text, vectors and access rights share epochs, so `as of epoch e` reconstructs exactly what an agent saw, and counterfactual runs evaluate against a fixed snapshot and write their derived facts back with provenance.
- **Graph-scoped retrieval.** Dense, type-prefixed IDs turn any graph predicate into a filter inside the vector index.
- **Open foundations.** Distributed, OSI-licensed, with GQL and SPARQL for interoperability.

**Deliberately not design targets:** schema-less workloads (open types exist for staging, not as the main mode), general-purpose transactional applications, and graph workloads whose value does not depend on a shared ontology.

## Decisions

| Question | Decision |
| --- | --- |
| Chunking and embedding generation | External pipelines; documents are pre-processed (chunked and embedded) before ingestion |
| Token-budget operator | No; per-branch `k` and `limit` are sufficient |
| MCP server tools | All three: introspection, projection functions and free-form UQL |
| First remote sources in version 2 | Defined on demand |

## Sources

- [Neo4j: Vector search with filters](https://neo4j.com/blog/developer/vector-search-with-filters-in-neo4j-v2026-01-preview/)
- [UltraGraph Schema and UQL](https://claude.ai/artifact/LtXDHoVcin1aGFJWDxZDbL)
- [UltraGraphDB Distribution Architecture](https://claude.ai/artifact/W7YBsh2yPKCLjehZiEsFZD)

* [PostgreSQL: postgres\_fdw](https://www.postgresql.org/docs/current/postgres-fdw.html)
* [oracle\_fdw](https://github.com/laurenz/oracle_fdw)
