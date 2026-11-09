## Why

UltraGraphDB stores quads under three catalog kinds, predicates, entity types and relations' roles, and its frontend
refuses an Install naming stage 6. `specification/schema/SCHEMA.md` specifies UGS, the one data definition language
every later stage reads: the query core of stage 7 compiles against its catalog, stage 8 classifies writes from it, and
the DeepBrain MVP is a UGS schema. Stage 6 builds UGS from its source text to the catalog, the store and the write
path, so that `specification/schema/deepbrain-gen5.ugs` installs and its data is created, read, updated and retracted
under every rule the schema declares.

The preparation is `openspec/notes/prepare-stage-6.md` (2026-11-08). This change follows it and departs from it in decisions of 2026-11-10, which the design records: schema
evolution moves whole to stage 9.5, a minimal subset of UQL carries the data operations, the Gen5 source is read from
`specification/schema/`, and the schema crates take a group folder of their own, `ultragraphdb_schema/`.

## Success

1. **The sample schema loads and is applied.** `specification/schema/deepbrain-gen5.ugs` installs into tenant 0 of a fresh
   cluster, the one tenant this stage has, through the binary protocol by the Rust client: no error, the compilation report holding exactly the warnings and
   notes the conformance corpus expects, Describe answering the source byte for byte and every catalog entry, and the
   schema id equal after a restart of every role.
2. **Simple CRUD works over the applied schema.** Through the UQL CRUD subset, in the tenant holding Gen5: an entity
   and a relation instance are inserted and read back; an update is read back as a revision; a retraction removes the
   instance from the default read and keeps it under `with retracted`; each write breaking a stage 6 rule is refused
   with its `UGS1xxx` number. After a Drop of everything in the tenant and an Install of the aviation schema of `SCHEMA.md`
   section 16, whose types are mutable, a delete removes an entity and leaves its tombstone.
3. `make test` is green, every group's mutation pass is done, and the fuzz runs are recorded.

Schema evolution, in place and by migration, is stage 9.5 (`openspec/notes/backlog.md` item 20), which has the full
cluster it needs: its success is a simple schema change installed over data and migrated together with that data.

## What Changes

- **The UGS compiler.** A lexer and parser for the whole grammar of `SCHEMA.md` sections 3 and 4, and the static
  phases 2 to 7 of section 10.1: resolution, typing, identity, relations and constraints, placement and
  classification, views. Every refusal carries its `UGS0xxx` diagnostic with location and fix. A construct that
  executes in a later stage is compiled, recorded and reported with `UGS0902`.
- **BREAKING — the catalog version.** The three-kind encoding and the preorder numbering of entity types give way to
  the catalog version of `SCHEMA.md` section 10.2: every entry in one canonical encoding, type prefixes allocated in
  declaration order from 3 and never reissued, prefix sets as sorted ranges, a schema id over the Describe body. The version's bytes are specified first, in a file of their own: the protocol's
  body layout, each entry self-contained, checksummed and verified when read, indexed by type prefix, with checks,
  limit conditions and computed derivations as verified postfix programs.
  Catalogs written before this stage are not readable; there are no outside users.
- **Install and Describe.** The frontend serves Install: the `declare` gate, the compile of the tenant's sources with
  those sent, prefix allocation, the version committed as pending, the reply with version, epoch, report and warnings,
  or code 28 with the diagnostics. A tenant takes one Install, of one or more
  sources compiled together; an Install into a tenant that holds a schema is refused naming stage 9.5. From its check
  until its version takes effect an Install fences the tenant's writes. Describe answers the sources and the typed entries across continuation frames.
- **`@index(ann)` declared and unenforced (gap G2 of the preparation).** An Install accepts a vector index
  declaration, records it and reports `UGS0902`; no index is built and a `Vector<BF16>` write is refused until
  stage 8.
- **Governed tenants.** A tenant holding a schema refuses undeclared data unless `@open` admits it; a tenant holding
  none keeps the write-path declarations of stage 5.5.
- **Identity at write.** An instance's ID takes its concrete type's prefix; keys take the canonical encoding of
  section 7.2, with bucket and claim from the key space; IRIs are minted from the key, by template, or as a handle;
  `rdf:type` of a declared type is answered from the prefix; relations are edge- or node-encoded by section 11.1.
- **Whole-instance validation at commit.** A transaction's quad changes are assembled into instances and each is
  validated against the catalog version in effect: every stage 6 row of `SCHEMA.md` section 8.2, bitemporal state,
  deletes with tombstones, measures, intervals, geometry. Every cross-partition case is refused with code 23.
- **Indexes and derived state.** `range`, `interval`, `geo` and `fulltext` indexes and materialized derivations within
  the anchor partition, written in the instance's local commit.
- **The RDF view.** Reads and writes through quads as `SCHEMA.md` sections 11.2 and 11.4 state, checked against
  `SCHEMA-APG.md` definition 17.1 by the Lean oracle's instance sets.
- **A UQL CRUD subset (new, decided 2026-11-10).** Query takes `language` 3 for `insert`, `update`, `delete`,
  `retract` and a root query by type and filter with a flat shape, the subset `QUERY-ULTRA-QL.md` marks for stage 6;
  every other UQL construct is refused with `UQL0901`. It exists so that stage 6 and the stages before the query core
  test data operations in the schema's own names.
- **Instance delete and retraction as commit-time operations.** Operation gains kinds 5, delete an instance, and 6,
  retract an instance, which the UQL subset compiles to and which every later query language uses.
- **BREAKING — Drop names its catalog version.** A Drop of a tenant's data or of everything in a tenant names the
  catalog version it drops under and is refused when that is not the current one. Until stage 9.5 it is how a tenant's
  schema is changed.
- **The wire.** `ultragraphdb_protocol_wire` brought to `PROTOCOL.md` as extended on 2026-11-08 and by this change's
  group 0: warnings, Describe's record kinds and continuation, Install's `staged`, Reference's get-only mode, codes 30
  and 31, datatypes 34 to 36, Purge and Migrate decoded and refused by stage, `catalog_bytes_max`. **BREAKING**: every
  golden frame of Result, Prepare, Describe, Install and Reference is regenerated.
- **The client.** `install`, `describe`, `query` for the UQL subset, the warnings of every reply, codes 30 and 31, the
  three datatypes, and a helper loading a folder of `.ugs` files into one Install.

## Capabilities

### New Capabilities

- `ugs-syntax`: the lexical structure, the grammar, the source bounds and the rendering of diagnostics.
- `ugs-semantics`: the static rules of a schema: resolution, typing, relations and constraints, placement and
  classification, view names, and the report with its later-stage notes.
- `ugs-identity`: type prefixes, prefix sets, canonical keys, key spaces, buckets, claims, instance IRIs and handles,
  vocabulary IRIs.
- `ugs-catalog`: the catalog version and its canonical encoding, the schema id, Install, Describe and the governed
  tenant.
- `ugs-enforcement`: instance assembly and the rules a commit holds: constraints, mutability classes, bitemporal
  statements, deletes, values, indexes and materialized derivations.
- `ugs-views`: relation encodings and the RDF view, read and written, with its oracle.
- `uql-crud`: the UQL subset of stage 6: its grammar, its typing against the catalog, its reads, its writes and its
  refusals.

### Modified Capabilities

- `system-identity`: the canonical key encoding covers UGS keys; type prefixes are allocated per declared type; a
  key's claim lives in the entity's own bucket; a keyless instance takes its handle's bucket.
- `catalog`: entity types take prefixes in declaration order in place of preorder numbers; the default entry of an
  undeclared predicate holds in an ungoverned tenant.
- `schema-storage`: a write to an unknown predicate records a default entry only in an ungoverned tenant or under
  `@open`.
- `mapping-contract`: the RDF view of a governed tenant is `SCHEMA-APG.md` definition 17.1; a declared type's
  `rdf:type` is answered from its prefix.
- `term-dictionary`: an IRI of a declared type's instance maps to an ID of that type's prefix; handle IRIs are minted
  into their type's space and held both ways.
- `metadata-group`: holds compiled UGS catalog versions and their sources, and allocates type prefixes; a version is
  bounded by `catalog_bytes_max` and travels in continuation frames; a drop of everything removes default-entry records.
- `schema-changes`: a drop of everything in a tenant removes its default-entry records; a governed tenant refuses the
  Drop of a predicate a schema declares.
- `wire-protocol`: Describe's record kinds and continuation frames; Query's `language` 3; Operation's kinds 5 and 6;
  warnings; codes 30 and 31; datatypes 34 to 36; Purge and Migrate decoded and refused by stage.
- `client-endpoint`: the frontend serves Install and a Query of the UQL subset; a vector index declared by Install is
  accepted and unenforced.
- `tokenizers`: a `measure` tokenizer ordering by the value in the kind's canonical unit.
- `scalar-types`: the datatypes `ugdb:bfloat16vector`, `ugdb:measure` and `ugdb:precision`.

## Impact

- **New crates:** `ultragraphdb_schema_syntax`, `ultragraphdb_schema_compile` and `ultragraphdb_schema_uql` in a new
  group folder, `ultragraphdb_schema/`, and `ultragraphdb_error_diagnostic`.
- **Changed crates:** `ultragraphdb_core_catalog`, `ultragraphdb_traits_catalog`, `ultragraphdb_core_ids`,
  `ultragraphdb_core_terms`, `ultragraphdb_core_mapping`, `ultragraphdb_core_tok`, `ultragraphdb_core_types`,
  `ultragraphdb_cluster_metadata`, `ultragraphdb_cluster_partition`, `ultragraphdb_common_frontend`,
  `ultragraphdb_common_compute`, `ultragraphdb_store_quads`, `ultragraphdb_store_apply`, `ultragraphdb_store_posting`,
  `ultragraphdb_transactions_txn`, `ultragraphdb_transactions_commit`, `ultragraphdb_protocol_wire`,
  `ultragraphdb_protocol_transport`, the three client crates, `ultragraphdb/ultragraphdb_tests`.
- **Specifications amended first (group 0):** `SCHEMA.md` section 14 and the `UGS0902` text (G2); `PROTOCOL.md`
  sections 7.3, 7.4, 8.4, 11.2 and 11.13; `QUERY-ULTRA-QL.md`'s stage 6 marks. The thirteen recommendations of
  `notes/specification-gaps.md` are applied to the specification files since 2026-11-10. `SCHEMA.md` already holds the resolution of 2026-11-10: a revision is
  refused under its rule's own number, and `UGS1025` names a required role's participant fixed for life.
- **Stored formats:** the catalog version's bytes and the Describe body change; stores written before this stage are
  not readable.
- **One new external dependency,** accepted by the user for now: the `regex` crate, whose syntax `SCHEMA.md` section
  6.6 names for `@pattern` (design decision 9).
- **Stage 9.5:** schema evolution in place and by migration, backlog item 20; namespace removal (gap G1 of the
  preparation) moves there. The module `sr` is left out of this stage.
