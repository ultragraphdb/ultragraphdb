## Why

SPARQL and GQL replace DQL (`openspec/notes/UltraGraphDB Dual Query Engine Plan.md`), and both engines read one store.
The store holds Dgraph's data model: predicates named by string, one schema type per predicate, values converted to
it, and keys laid out byte for byte as Dgraph's. RDF needs terms kept exactly as written, objects of any kind under
one predicate, named graphs and statement identity, and the property-graph view needs the same data seen as nodes,
edges and properties. Stage 4 builds that data model under the storage of stage 2, before stage 5's transactions and
stage 6's query core depend on it.

## What Changes

- **The mapping contract v1:** the canonical statement model, quads with an optional statement ID, and its RDF and
  property-graph views, written as a versioned specification with round-trip tests.
- **The catalog v1:** predicates, entity types with single inheritance and abstract types, relations with named roles,
  cardinality, datatype constraints, unique values and indexes, persisted per namespace. An undeclared predicate gets a
  default entry: multi-valued, any object.
- **The term dictionary:** every IRI and blank node gets an allocated 64-bit UID whose top 16 bits are reserved for an
  entity-type prefix; every literal is encoded by content, by value only when its lexical form is canonical, so RDF
  term identity stays exact. Vectors, of the system datatype `ugdb:float32vector`, are encoded by value at any length
  and take no dictionary entry and no reverse entry. The dictionary lives in a keyspace without versions, which fjall's
  bloom filters serve.
- **The quad layout:** forward, reverse, subject-predicate, object-predicate, value-index, statement and statistics
  keyspaces, each keyed by namespace and graph and by term IDs instead of attribute strings, with a region identity
  for every key.
- **Changesets** in place of Dgraph's `DirectedEdge`: quad inserts and deletes, deletes by pattern, single-valued
  overwrites and statement records, applied through index maintenance and driven by the catalog.
- **Writes without reads where RDF allows them:** inserting into a multi-valued predicate, its reverse, predicate-list
  and index entries is a blind delta; reads remain for deletes, single-valued overwrites, unique values and exact
  counts. A term new to a write gets a fresh ID, whose lists are known empty without a read.
- **A data dictionary:** one read API over the catalog, the statistics and per-label shapes, which every later interface
  answers from: the native describe request (stage 5), SPARQL service descriptions, VoID statistics and GQL catalog
  queries (stage 8), and Bolt's schema procedures (stage 9) (`db.labels()`, `db.schema.nodeTypeProperties()`,
  `apoc.meta.data()` and the rest), which LangChain and LlamaIndex call before their first query. Shapes of undeclared
  data come from a background pass, so writes stay read-free.
- `store_bench` loads quads and reports list reads per relation.
- **BREAKING:** DQL, Dgraph's mutation formats and the Dgraph gRPC client are removed: `ultragraphdb_core_dql`,
  `ultragraphdb_core_lex`, `ultragraphdb_core_chunker`, `ultragraphdb_core_facets` and `ultragraphdb_client`. The DQL
  schema parser in `ultragraphdb_core_schema` goes, and that crate becomes the catalog.
- **BREAKING:** keys are no longer byte-compatible with Dgraph's; the key golden vectors retire. Data from Dgraph
  enters through its RDF export and stage 7's loaders, as before.

## Capabilities

### New Capabilities

- `mapping-contract`: the canonical statement model and its RDF and property-graph views, versioned.
- `catalog`: the declarations both engines and index maintenance read, their persistence and their defaults.
- `term-dictionary`: how IRIs, blank nodes and literals become term IDs, and the keyspace without versions that maps
  them.
- `quad-storage`: the keyspaces quads are stored in, named graphs, statement IDs, the UID type bits, regions and
  statistics.
- `changesets`: the write format engines produce and index maintenance applies.
- `data-dictionary`: the read API that answers what the data holds: labels, relationship types, property keys,
  per-label shapes, declarations, indexes and statistics.

### Modified Capabilities

- `key-encoding`: the quad layout replaces Dgraph's byte layout; namespaced attribute names and Dgraph's reserved
  predicates go.
- `kv-store`: a keyspace without versions, written atomically with the versioned keyspaces.
- `storage-simulation`: the simulated store models the keyspace without versions under the same faults and shadow
  model.
- `index-maintenance`: driven by the catalog and changesets; set semantics; reads only where a write needs them; terms
  kept as written instead of converted to a schema type; each tokenizer indexes the datatypes it takes, from the value.
- `posting-lists`: a literal posting is identified by its encoded term; cardinality comes from the catalog.
- `schema-storage`: the catalog persists in place of the DQL schema, and an undeclared predicate gets a default entry.
- `schema-changes`: catalog changes in place of DQL type changes; drops of graphs; drops cover every quad keyspace.
- `vector-index`: the HNSW lists live in the quad layout; vectors are `ugdb:float32vector` literals encoded by value.
- `port-testing`: golden vectors from Go remain only where an output must still equal Dgraph's.

### Removed Capabilities

- `dql-parsing`, `schema-parsing`, `mutation-input`: DQL, its schema language and Dgraph's mutation formats.

## Impact

- **Crates:**
  - rewritten: `ultragraphdb_core_keys`;
  - `ultragraphdb_core_schema` renamed to the catalog crate;
  - changed: `ultragraphdb_traits_store`, `ultragraphdb_store_kv`, `ultragraphdb_store_sim`,
    `ultragraphdb_store_posting`, `ultragraphdb_store_apply`, `ultragraphdb_store_hnsw`, `ultragraphdb_tool_bench`;
  - new: the term dictionary, the quad store and the mapping contract;
  - removed: the five listed above, with the user's approval at the task that removes them.
- **Dependencies:** `oxrdf` and `oxsdatatypes` (MIT OR Apache-2.0, pure Rust) for the RDF term model and XSD values;
  `blake2` is already a workspace dependency.
- **Kept, narrowed:** `ultragraphdb_proto_dgraph` stays as a message dependency of `ultragraphdb_proto_pb`, whose
  `pb.proto` imports `api.proto`; nothing uses its gRPC service after this stage.
- **Tests:** DQL, schema-parsing and mutation-input suites and the Dgraph key golden vectors retire; the posting,
  apply, HNSW and simulator suites move to the new keys and the catalog.
