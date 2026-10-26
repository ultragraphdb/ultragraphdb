Every task group runs in the same order: write the group's feature scenarios, verbatim from the specs, and its tests
(they fail), write the code (they pass), refactor with both green. A group is done when its BDD suite and unit tests
pass under Cargo and Bazel with equal counts, and its operations run in the simulator's workload under every fault
class.

## 0. Workspace

- [x] 0.1 Add `oxrdf` and `oxsdatatypes` (MIT OR Apache-2.0, pure Rust) to `[workspace.dependencies]`, pinned, each with the features its use needs and no others; record their versions and why in the design's crate table
- [x] 0.2 Create the crates `ultragraphdb_core_terms`, `ultragraphdb_core_mapping` and `ultragraphdb_store_quads` with the repository's crate layout, headers and Bazel targets, hermetic and sandboxed

## 1. The keyspace without versions → `ultragraphdb_traits_store`, `ultragraphdb_store_kv`, `ultragraphdb_store_sim`

- [x] 1.1 Scenarios: kv-store ("A keyspace without versions"), storage-simulation ("The simulated store models the keyspace without versions")
- [x] 1.2 The `Store` trait's versionless point read and write-batch entries, as required methods
- [x] 1.3 fjall: the `terms` keyspace beside `data` and `meta`, in the same `OwnedWriteBatch` and `Database::snapshot()`, cleared by `drop_all`, reads not bounded by the discard timestamp; the kill harness covers a batch with versionless entries
- [x] 1.4 The simulator: the versionless map under every fault class; the shadow-model rule that a batch's versionless entries are visible exactly when its versioned ones are

## 2. Terms → `ultragraphdb_core_terms`

- [x] 2.1 Scenarios: term-dictionary ("Literals are encoded by content")
- [x] 2.2 The term model on `oxrdf`; `EncodedTerm` with the tag table of the design; encoding by value only for canonical lexical forms, checked by an `oxsdatatypes` round trip
- [x] 2.3 Inline small strings, language tags and typed literals; BLAKE2b-128 hashes for the rest; `ugdb:float32vector` by value with its dimension, refused unless canonical, empty-free, finite and at most 65,535 elements; decoding, with the dictionary supplying hashed strings; equal terms encode equally, unequal terms unequally, over every XSD type and directional strings
- [x] 2.4 The posting fingerprint: farm `Fingerprint64` of the encoding, injectable for tests

## 3. Quad keys → `ultragraphdb_core_keys`, rewritten

- [x] 3.1 Scenarios: key-encoding (the modified and added requirements), quad-storage ("The quad keyspaces": key order, literal objects)
- [x] 3.2 Builders for every keyspace of the design's table; objects as self-delimiting UIDs or encodings; split keys with the `0x80` flag and the part's start
- [x] 3.3 Parsing every keyspace back into its fields; refusal of zero UIDs and of every truncation without a panic, tested by parsing every proper prefix of each key kind
- [x] 3.4 `Region` from a key, for every keyspace
- [ ] 3.5 With the user's approval, retire the Dgraph key golden vectors and the generator files that produce only them; list each file removed

## 4. The mapping contract → `ultragraphdb_core_mapping`

- [x] 4.1 Scenarios: mapping-contract, every requirement
- [x] 4.2 The canonical statement, statement IDs and the rule for allocating one; the contract version
- [x] 4.3 The RDF view: quads from statements, one quad for parallel edges, reifiers and annotations for statement IDs and their properties; skolem IRIs under `/.well-known/genid/`, flagged, presented as blank nodes
- [x] 4.4 The property-graph view: nodes, labels from `rdf:type`, properties scalar or list by the catalog's cardinality, edges, parallel edges, edge properties; names mapped to IRIs under the base IRI
- [x] 4.5 `Changeset`: inserts, deletes, deletes by pattern, statement-property sets and deletes, catalog declarations, in order, with RDF terms
- [x] 4.6 Round-trip tests through both views over named graphs, blank nodes, language-tagged, directional, typed and ill-typed literals, parallel edges and edge properties

## 5. The catalog → `ultragraphdb_core_catalog`

- [ ] 5.1 Move `ultragraphdb_core_schema` to `ultragraphdb_core/ultragraphdb_core_catalog` with `git mv` and rename the crate; every dependent builds
- [ ] 5.2 Scenarios: catalog, schema-storage
- [ ] 5.3 Predicate declarations (kind, cardinality, datatype constraint, unique values, tokenizers, exact count index, vector index and its parameters), entity types with one parent and the abstract flag, relations with named roles; the default for an undeclared predicate; the initial catalog declaring `rdf:type` an edge
- [ ] 5.4 The write checks: object kind, datatype constraint, single-valued overwrite, unique values by one read of the predicate's reverse list for the object; a declaration whose datatype constraint none of its tokenizers takes refused
- [ ] 5.5 Preorder numbering of the type hierarchy; refusal of a second parent and of a cycle
- [ ] 5.6 Persistence per namespace in catalog keys with the discard flag; load at open; removed declarations stay removed; overlays local to a write; a default entry written in the write's commit batch
- [ ] 5.7 With the user's approval, remove the DQL schema parser and its tests once the catalog's suites pass; list each file removed

## 6. The dictionary and the quad store → `ultragraphdb_store_quads`

- [ ] 6.1 Scenarios: term-dictionary (the remaining requirements), quad-storage (the remaining requirements)
- [ ] 6.2 The dictionary over the `terms` keyspace: IRI to UID by BLAKE2b-128, UID to IRI with the skolem flag, hash to string; entries written only in commit batches; literal entries written blind; a missing entry reported as corruption naming its key
- [ ] 6.3 UID and statement-ID leasing: the high-water mark synced in `meta` before a block is handed out, the type bits zero, UID 0 never assigned; a crash test that no UID is reissued
- [ ] 6.4 New IRIs: provisional UIDs in the write, assignment in the commit hook, refusal with `Transaction has been aborted. Please retry` when a commit since the write's read assigned the same IRI
- [ ] 6.5 Quad lists on the posting crate: forward, reverse (no vector objects), subject and object predicates (no vector objects), statement records keyed by graph and statement ID, the statement-ID field on `Posting`, splits on every list keyspace
- [ ] 6.6 Literal postings: the fingerprint in `uid`, postings ordered by encoding after equal fingerprints, reads comparing encodings; collisions forced through the injected fingerprint
- [ ] 6.7 Statistics by deltas, never read by a write
- [ ] 6.8 Scenarios: data-dictionary; quad-storage ("Statistics are kept by deltas": counts per object kind and datatype)
- [ ] 6.9 The data dictionary's read API: labels, relationship types, property keys, per-label shapes, declarations, indexes, statistics; declared entries from the catalog; per-label shapes from a background pass over a snapshot, replaced whole, with its snapshot timestamp; a test that no write reads or updates a shape
- [ ] 6.10 The simulator workload: terms created, quads written and deleted, under every fault class; every stored UID and hash has its dictionary entry after every step and every recovery

## 7. Changesets through index maintenance → `ultragraphdb_store_apply`

- [ ] 7.1 Scenarios: changesets, index-maintenance, posting-lists (the modified requirements, rewritten from DQL into catalog declarations)
- [ ] 7.2 Applying a changeset as one `Request`: term resolution, the catalog checks, whole or nothing
- [ ] 7.3 Index maintenance on the quad keys: value-index tokens by language, shared tokens kept on delete, mandatory reverse entries, predicate lists, the exact count index in both directions, deletes by pattern
- [ ] 7.4 Reads only where needed: `get_from_delta` for multi-valued inserts, `get` for deletes, single-valued overwrites, unique predicates and counted predicates; a test that an insert reads no list and that a write naming a new UID reads none of its lists
- [ ] 7.5 Terms kept as written; literals converted only to compute index tokens; the tokenizer-datatype table of the index-maintenance spec, tokens from values, other datatypes held unindexed; `geo` over `geo:geoJSONLiteral` through the ported GeoJSON parser
- [ ] 7.6 The read and written regions of a changeset reported; determinism: one changeset applied to two copies writes the same bytes
- [ ] 7.7 The simulator workload of stage 2 rewritten to changesets, every invariant kept

## 8. Catalog changes, rebuilds and drops → `ultragraphdb_store_apply`, `ultragraphdb_store_hnsw`

- [ ] 8.1 Scenarios: schema-changes, vector-index (the modified requirements)
- [ ] 8.2 The rebuild decision tables over tokenizers, the count index and the vector index; `needsReverseEdgesRebuild` removed; the two-phase rebuild, its temporary keyspace and rebuild record on the quad prefixes
- [ ] 8.3 Catalog changes refused against the data: single-valued with a subject holding two objects, an edge holding literals, a property holding IRIs, a datatype constraint the data contradicts; applied otherwise
- [ ] 8.4 Drops: a predicate across every keyspace and graph, its statement records and its entries in the predicate lists; a graph; a namespace's data; everything in a namespace and everything, with the initial catalog at timestamp 1; dictionary entries kept
- [ ] 8.5 The HNSW lists in the vector-index keyspace, the search reading elements from the vector's encoding and refusing another dimension; the golden replays compare values under a translation from Dgraph's keys

## 9. DQL removed

- [ ] 9.1 With the user's approval: remove `ultragraphdb_core_dql`, `ultragraphdb_core_lex`, `ultragraphdb_core_chunker`, `ultragraphdb_core_facets` and `ultragraphdb_client`, their Bazel targets and workspace entries; list each directory removed
- [ ] 9.2 The specs `dql-parsing`, `schema-parsing` and `mutation-input` removed at archive; no feature file names them

## 10. Close the stage

- [ ] 10.1 `store_bench` generates its relation commits as quads and reports list reads per relation, cache hits and reads that found nothing; one run per configuration in OrbStack, recorded in the overview beside stage 2's numbers
- [ ] 10.2 Every scenario of this stage has its feature; `scripts/bdd_counts.sh` reports equal counts; `make test` green; `make fix` and `make format` clean
- [ ] 10.3 Headers on every new file; `AGENTS.md` crate and external-dependency tables updated; the ledger's rows that name removed crates marked
