## Context

Stages 1 and 2 ported Dgraph's foundations and storage: posting lists over fjall at commit timestamps, index
maintenance, rebuilds, drops, the HNSW index and the deterministic simulator, all keyed by Dgraph's byte layout and
typed by Dgraph's schema. `openspec/notes/UltraGraphDB Dual Query Engine Plan.md` replaces DQL with SPARQL and GQL over
one store and sets out stage 4: the data model both engines share, and the storage beneath it. Stage 5 then builds the
commit step on n−1 cores, stage 6 the query core and stage 8 the full engines; each reads this stage's keys, terms,
catalog and changesets unchanged, so their shapes are decided here, including what stage 10's tiles and log carry
("Decided ahead, planned per stage" in the plan).

The writes still go through stage 2's commit path in this stage: one writer, `Lists::commit_with`, its hook under the
writer lock.

## Goals / Non-Goals

**Goals:**
- The mapping contract v1 and its two views, with round-trip tests.
- The catalog v1, persisted, read by index maintenance and the write checks.
- Term encoding and the dictionary in a keyspace without versions, in fjall and in the simulator.
- The quad keyspaces, statement IDs, the reserved UID type bits, regions and statistics.
- Changesets applied through index maintenance, with writes that read only what they need.
- DQL and Dgraph's mutation formats and client removed; every remaining suite on the new keys.
- `store_bench` on quads, reporting list reads per relation.

**Non-Goals:**
- Transactions on n−1 cores, read summaries, epochs (stage 5).
- The binary protocol (stage 5); the query core (stage 6); full SPARQL and GQL (stage 8); Bolt (stage 9).
- RDF file loading (stage 7); SHACL and GQL graph-type syntax (stage 8).
- Entity-type prefixes inside UIDs: the bits are reserved, not used (stage 10).
- Removing dictionary entries (stage 10).

## Decisions

### Crates

| Crate | Path | Depends on | Provides |
|---|---|---|---|
| `ultragraphdb_core_terms` | `ultragraphdb_core/ultragraphdb_core_terms` | `oxrdf`, `oxsdatatypes`, `blake2`, `ultragraphdb_common_utils` | The term model on `oxrdf`; `EncodedTerm` and its encoding, decoding and canonical-form check; the 128-bit hash; the posting fingerprint |
| `ultragraphdb_core_keys` | rewritten in place | `ultragraphdb_core_terms` | The quad keys, their parsing, split keys and regions |
| `ultragraphdb_core_mapping` | `ultragraphdb_core/ultragraphdb_core_mapping` | `ultragraphdb_core_terms`, `oxrdf` | The canonical statement, statement IDs, the RDF and property-graph views, `Changeset` and the declaration values it carries, the contract version, and `CanonicalDataset`, the canonical statements in memory that a store is checked against |
| `ultragraphdb_core_catalog` | `ultragraphdb_core_schema`, moved with `git mv` and renamed | `ultragraphdb_core_keys`, `ultragraphdb_core_terms`, `ultragraphdb_core_tok`, `ultragraphdb_traits_store` | Declarations, defaults, the write checks, persistence, preorder numbering; the DQL schema parser removed |
| `ultragraphdb_store_quads` | `ultragraphdb_store/ultragraphdb_store_quads` | `ultragraphdb_store_posting`, `ultragraphdb_core_keys`, `ultragraphdb_core_terms`, `ultragraphdb_core_mapping`, `ultragraphdb_traits_store` | The dictionary over the keyspace without versions, UID and statement-ID allocation, quad reads and writes on posting lists, statistics |
| `ultragraphdb_store_apply` | changed | adds `ultragraphdb_store_quads`, `ultragraphdb_core_catalog`, `ultragraphdb_core_mapping` | Changesets applied through index maintenance, rebuilds and drops on the catalog |

`oxrdf` 0.3.5 with `rdf-12`, for RDF 1.2's directional language-tagged strings, and `oxsdatatypes` 0.2.4 with no
features: both MIT OR Apache-2.0 and pure Rust, Oxigraph's own term model and XSD values, so the canonical test below is
the round trip SPARQL engines built on them use.

`ultragraphdb_core_mapping` holds `Changeset` because it is the contract between the engines and storage: stages 6 and 8
produce changesets without depending on a storage crate. A changeset carries catalog declarations, so their values are the mapping crate's
too; the catalog crate holds their state, persistence and checks.

**Removed, with the user's approval at the task that removes them:**
- `ultragraphdb_core_dql`;
- `ultragraphdb_core_lex`, used only by the DQL and schema parsers;
- `ultragraphdb_core_chunker`;
- `ultragraphdb_core_facets`, used only by the chunker;
- `ultragraphdb_client`.

`ultragraphdb_proto_dgraph` stays: `pb.proto` imports `api.proto`, and `ultragraphdb_core_types` and
`ultragraphdb_store_posting` name its messages. Its gRPC service is no longer used.

### The keyspace without versions

The `Store` trait gains required methods: a point read of a versionless key, and write-batch entries that put or remove
one. fjall holds them in a third keyspace, `terms`, beside `data` and `meta`, so one `OwnedWriteBatch` stays atomic
across all three. fjall's point gets consult each table's bloom filter (`lsm-tree-3.1.10/src/table/mod.rs:281`), which
is the point: an absent term costs a filter probe. Reads of the keyspace go through the same `Database::snapshot()` as
the others, but no discard timestamp bounds them, since nothing in it has versions. `drop_all` clears it with the others.

The simulator holds it as a map beside its versioned contents. Every fault applies to a batch's versionless entries
exactly as to its versioned ones, and the shadow model checks the two halves stay together.

Alternative: keep terms in the versioned keyspace. Rejected: every lookup of an absent term would be a range seek,
which is the cost stage 2 measured at 34 % of the apply thread.

### Terms

**UIDs:**
- **Width:** 64 bits; the top 16 are the entity-type prefix, zero in this stage.
- **Allocation:** from leased blocks. The high-water mark of a lease is written, synced, in the `meta` keyspace before
  any UID of the block is handed out, so a crash never reissues a UID. That is the scheme Dgraph's Zero leases with.
- **Statement IDs:** a sequence of their own, per namespace, leased the same way.

**The dictionary**, in the `terms` keyspace, per namespace:

| Key | Value |
|---|---|
| `ns`, `0x01`, BLAKE2b-128 of the IRI | UID |
| `ns`, `0x02`, UID | IRI, and a flag marking a skolemized blank node |
| `ns`, `0x03`, BLAKE2b-128 of a string | The string |

- **IRIs are hashed** for a fixed key width. A 128-bit collision between two IRIs is not handled.
- **Blank nodes are skolemized** as IRIs under the base IRI's `/.well-known/genid/` path, which RDF 1.1 recommends for
  skolem IRIs, and flagged so the RDF view presents them as blank nodes again.

**A new IRI** is looked up by the write, and its UID is assigned in the commit hook under the writer lock. If a commit
since the write's read assigned the same IRI, the hook refuses the write with `Transaction has been aborted. Please
retry`, as the term-dictionary spec requires; the retry finds the UID. Stage 5 keeps this rule in its commit step.

**Literal encoding:** one tag byte and at most 32 bytes.

| Tag | Holds |
|---|---|
| boolean, integer (i64), decimal (16 bytes), double, float, date/time/duration kinds | The value, when the lexical form is canonical |
| small string; small language string; small directional string, LTR and RTL | Lexical form and language tag, each at most 15 bytes, inline |
| big string; big language string; big directional string | 128-bit hashes of the lexical form, and of the language tag when it is over 15 bytes |
| small typed literal; big typed literal | Datatype UID and lexical form, inline or hashed |
| vector | `ugdb:float32vector`: the dimension as a u16 and the elements as little-endian f32, at whatever length |

- **The canonical test:** parse the lexical form with `oxsdatatypes`, format the value back, and compare. Only an
  identical round trip is encoded by value. Oxigraph encodes by value whatever parses (`parse_integer_str`), which
  merges `"01"` and `"1"`; the term-dictionary spec forbids that.
- **The hash:** BLAKE2b truncated to 128 bits, not Oxigraph's fixed-key SipHash, because the input is untrusted.
- **The posting fingerprint:** farm `Fingerprint64` (`ultragraphdb_common_utils::go_farm`) of the encoding.
- **Vectors** are the one encoding past 33 bytes. A DeepBrain embedding of 768 elements has a lexical form of several
  KB; hashed, every embedding written would leave a dictionary entry until stage 10 removes entries, and every HNSW
  distance would fetch and parse text. By value, the posting holds the floats the search reads, as Dgraph's
  `float32vector` posting does. The datatype accepts only canonical lexical forms, so term identity stays exact.
  Vectors take no reverse or object-predicate entry: no query looks a subject up by its embedding, and a bound vector
  object is answered by a scan of the predicate's forward lists. `ugdb:` abbreviates the system vocabulary, whose base
  IRI is open ("Open Questions").

### The quad keys

Every key starts with the keyspace byte and the namespace as a u64 big-endian. The split flag is `0x80` on the keyspace
byte, with the part's start appended.

| Byte | Keyspace | Rest of the key |
|---|---|---|
| `0x01` | catalog | entry kind, then the predicate's or type's UID |
| `0x10` | forward | graph, predicate, subject |
| `0x11` | reverse | graph, predicate, object |
| `0x12` | subject predicates | graph, subject |
| `0x13` | object predicates | graph, object |
| `0x14` | value index | graph, predicate, tokenizer id, token |
| `0x15` | count index | graph, predicate, direction, count (u32) |
| `0x16` | vector index | graph, predicate, list kind, node |
| `0x17` | statements | graph, statement ID |
| `0x18` | statistics | graph, predicate |

- **UIDs** appear as u64 big-endian.
- **An object** appears as the UID tag and its UID, or as a literal's encoding. Both are self-delimiting, so a key with
  a literal object parses without a length field. No key carries a vector.
- **Regions** are key prefixes: the keyspace byte, namespace, graph and, where the keyspace has one, predicate. A
  `Region` type computed from a key is all stage 5 needs.

### Postings

The forward list of (graph, predicate, subject) holds one posting per object:
- **an IRI or blank-node object:** its UID, as Dgraph's reference postings;
- **a literal:** a value posting whose value is its encoding and whose `uid` field is its fingerprint;
- **parallel edges:** one posting for the object, carrying every statement ID of the edge in a repeated field added to
  `pb.proto`'s `Posting`. The posting format is the port's own once Dgraph compatibility ends.

That one posting per object keeps the UID pack strictly increasing, as stage 2's codec requires. Two literals with
equal fingerprints share that pack entry, and their postings sort by encoding after it. Reads by fingerprint compare
encodings, so a collision never merges values. A test injects the fingerprint function to force collisions.

### Writes

A changeset is applied as one `Request` of the posting crate:
1. Its terms are resolved through the dictionary; terms new to the write get provisional UIDs, assigned in the hook.
2. Its operations are checked against the catalog; one refusal refuses the changeset.
3. Each operation maintains its lists. Inserts into multi-valued predicates use `get_from_delta`, so they read nothing.
   Deletes, single-valued overwrites, unique predicates and count-indexed predicates read with `get`, which keeps ledger
   35's rule that a deleted value's tokens survive when other values still produce them.
4. A UID the write created is known empty, so none of its lists is read.
5. The read and written keys are mapped to regions and reported.

The posting crate's batch model is unchanged in this stage. Stage 5 replaces it.

### The catalog

`SchemaState`'s machinery carries over: load and persist per namespace with the discard flag, overlays local to a
write, the interim declaration during a rebuild.
- **The rebuild decision tables** compare tokenizers, the count index and the vector index. `needsReverseEdgesRebuild`
  goes: reverse lists are always kept.
- **An undeclared predicate** gets the default entry in the write's commit batch. Concurrent first writes produce
  identical entries, so stage 2's rule that the first commit's schema wins and later commits convert is no longer
  needed.
- **A unique predicate** holds an object for one subject at most within a graph. The write check reads the predicate's
  reverse list for the object, one list read, which stage 5's validation then covers; scrith's `epi.id` needs it
  (`openspec/notes/UltraGraphDB Dual Query Engine Plan.md`, "What scrith's ontology needs from the single node").
- **Tokenizers take datatypes** as the index-maintenance spec's table states. Dgraph converted every value to the
  predicate's one type before tokenizing; here a predicate's objects are terms of any datatype, so each tokenizer names
  the ones it takes, tokens come from the value, and a literal of another datatype is held unindexed, which SPARQL's
  comparisons already treat as an error that filters it out. DeepBrain's ranges over `time.periods`, `space.cells` and
  `time.valid_from` read the `int` and `day` indexes. The `geo` tokenizer reads GeoJSON, which the ported parser
  (`ultragraphdb_core_types/src/types/geometry/geometry_geojson.rs`) already reads, so `geo:geoJSONLiteral` is indexed
  from this stage and `geo:wktLiteral` waits for a WKT parser in stage 8.
- **The initial catalog** of a namespace declares `rdf:type` as an edge, with nothing else.

### The data dictionary

One read API in `ultragraphdb_store_quads` answers what the data holds; stages 5, 8 and 9 expose it through their own
interfaces and add nothing to it:

| Stage | Interface |
|---|---|
| 5 | The binary protocol's describe request |
| 8 | SPARQL 1.1 Service Description and VoID statistics; GQL catalog queries |
| 9 | Bolt's `db.labels()`, `db.relationshipTypes()`, `db.propertyKeys()`, `db.schema.nodeTypeProperties()`, `db.schema.relTypeProperties()`, `db.schema.visualization()`, `dbms.components()`, the `SHOW` commands, and an `apoc.meta.data()`-shaped result, with Neo4j's exact names, columns and value types |

- **Declared entries answer exactly,** from the catalog. Neo4j derives `nodeTypeProperties` by sampling, so its answer
  can be incomplete.
- **Per-predicate kinds and datatypes** come from the statistics, which writes keep exact by deltas without reading.
- **Per-label shapes of undeclared data** come from a background pass over a snapshot. It joins each subject's
  `rdf:type` statements with its subject-predicate list, and for edges with the object's types. It replaces the shapes
  whole and records its snapshot timestamp. Keeping shapes exact on every write would need a read of the subject's labels
  per insert, which the write path avoids.

### Tests

- **No golden vectors from Go** for keys, terms, the mapping contract or the catalog; they are the port's own.
- **Scenarios** come verbatim from the specs.
- **Round trips** run over `oxrdf` quads, and over property graphs built through the view.
- **The simulator** gains a dictionary workload: terms created, quads written, faults injected.
- **The HNSW golden replays** compare values under a translation from Dgraph's keys to the vector-index keys, instead
  of comparing store contents byte for byte.
- **`store_bench`** generates its Dgraph-shaped relation commits as quads.

## Risks / Trade-offs

- [The scenarios of four existing specs are rewritten from DQL schema syntax into catalog declarations] → Each
  rewritten scenario keeps the behaviour it tested; the delta specs carry them, and the old feature files are rewritten
  in the same task as their code.
- [Literals with colliding fingerprints share one UID-pack entry] → The collision path is tested with an injected
  fingerprint, under the simulator as well.
- [Large tested code is removed, about 9,000 lines of DQL tests among it] → It is removed only after the replacement
  suites pass, at a task that asks for the user's approval.
- [A gap in the mapping contract found after this stage changes the layout] → The round-trip tests cover both views,
  parallel edges and edge properties; stage 6's GQL core is the first to stress the property-graph view.
- [`store_bench`'s numbers are not comparable with stage 2's] → The quad workload is recorded as a new baseline beside
  the old one.

## Migration Plan

There are no outside users. Stores written by stage 2 are not readable: the mapping contract's version is recorded at
creation, and a store without one, or with another major version, is refused at open. Data from Dgraph enters through
its RDF export and stage 7's loaders.

## Open Questions

- What is the base IRI of the system vocabulary that `ugdb:` abbreviates, which names `ugdb:float32vector` and the
  system's other IRIs?
- What is a namespace's default base IRI, before an administrator sets one?
