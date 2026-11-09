## Context

`specification/schema/SCHEMA.md` is the specification of UGS, `SCHEMA-APG.md` its formal semantics,
`specification/binary_protocol/PROTOCOL.md` the bytes, and `openspec/notes/prepare-stage-6.md` the preparation of
2026-11-08: the state stage 5.5 left (its section 3), the cut (section 4), the groups (section 6) and their order
(section 7). This design is read with that note; it restates what changes and records what the user decided on
2026-11-10. Where the two differ, this change holds.

Where stage 6 starts, checked against the repository on 2026-11-10:

| Area | State |
|---|---|
| UGS in code | No crate parses UGS. The frontend refuses an Install naming stage 6 (`PROTOCOL.md` section 11.2) |
| Catalog | Three kinds: predicate declarations, entity types numbered in preorder, relations' roles (`catalog` specification) |
| Wire | `PROTOCOL.md` carries Install, Describe's twenty entry kinds with continuation, warnings, codes 30 and 31, datatypes 34 to 36, Purge and Migrate, and Reference's get-only mode; `ultragraphdb_protocol_wire` implements the frames of 2026-11-05 |
| Writes | The frontend relays a client's Write, Operation, Commit and Abort frames unchanged on the Bind stream of the transaction's partition (`PROTOCOL.md` section 11.12). Change records (section 4.6) insert and delete statements and properties; Operation (section 7.3) has kinds 1 to 4 |
| Query | Query (22) names `language` 1, GQL, or 2, SPARQL, and is refused naming stage 7; section 9.4's language table already numbers UQL 3 and UGS 4 |
| The APG model | `lean/schema/ApgModel` with its oracle (`Oracle/Aviation.lean`, `Oracle/Gen5.lean`), the preparation's group A, tasks A.1 to A.5 |
| The specification gaps | G2 (`@index(ann)` declared and unenforced) is closed in `SCHEMA.md` section 14 and `PROTOCOL.md` section 11.13 since 2026-11-10. G1 (namespace removal) moves to stage 9.5 with evolution. The thirteen gaps found while drafting are closed in the specification files (`notes/specification-gaps.md`, decision 13) |
| The sample | `specification/schema/deepbrain-gen5.ugs`: one `schema deep_brain version 5` source of 975 lines, 76 entity, 87 relation and 2 statement types, 6 derivations, 14 functions, 89 indexes, one graph. Every type sits under `Statement @bitemporal` |

Constraints: correctness, speed, scale and developer experience in that order (`AGENTS.md`); no panics; static
dispatch; Bazel hermetic; every scenario of this change a Gherkin scenario in its owning crate; no outside users, so
formats change in place.

## Goals / Non-Goals

**Goals:**

- The whole language accepted and every construct of `SCHEMA.md` section 14's stage 6 rows enforced.
- `deepbrain-gen5.ugs` installs and its data is created, read, revised and retracted under its rules.
- One compiled form, the catalog version, read by the metadata group, the partitions, compute and the frontend.
- A way to run data operations in the schema's names before stage 7: the UQL CRUD subset.
- The commit path stays lean: whole-instance validation with allocation-free catalog lookups.

**Non-Goals:**

- Schema evolution, all of it: a second Install into a tenant, the changes in place of `SCHEMA.md` section 13, the
  validation scan, deprecate and drop, namespace removal (G1), migrations. Stage 9.5, backlog item 20.
- The query core: GQL, SPARQL, UQL beyond the subset, functions and virtual derivations executed, the GQL view, SHACL
  export. Stage 7.
- Everything across partitions: claims at another partition, coordinated and segmented paths, shares at referrers,
  Purge, `@index(ann)` built, `Vector<BF16>` stored. Stage 8.
- Grants. The gate allows every authenticated principal, as in stage 5.5.
- The module `sr` (`ServiceRadarExtension.ugs`). Modules are compiled and tested from the corpus's own sources.

## Decisions

### 1. The scope is `SCHEMA.md` section 14's stage 6 column, whole (user, 2026-11-10)

The success criteria name Gen5 and simple CRUD; the scope is every stage 6 row. Gen5 uses no mutable type, no
`@on_delete`, no `@open`, no `@values` and no `@valid`; the aviation schema of section 16 and the conformance corpus
carry those.

- *Alternative, a Gen5-driven cut:* smaller, and it leaves a language that refuses the specification's own worked
  example and the several schemata stage 6.7 tests transactions against.

### 2. Evolution is stage 9.5; a tenant takes one Install (user, 2026-11-10)

Every change to an installed schema waits for stage 9.5, which has the full cluster: a change takes effect at an
epoch in every partition holding the tenant, a tightening scans them, and a migration moves data between them. In
stage 6 a tenant takes one Install, of one or more sources compiled together: schemas, and the modules extending
them. An Install into a tenant that holds a schema is refused with code 7, the message naming stage 9.5, as the
frontend refuses every command of a later stage. The `version` an Install builds on is checked first, and one no
longer current is refused with code 2, kind Catalog (`PROTOCOL.md` section 8.4). From its check of the tenant until its
version takes effect, an Install fences the tenant's writes with code 31, so no data is written ungoverned in between
(user, 2026-11-10).

- *Consequence:* a schema under development is installed into a fresh tenant, and a tenant's schemas are removed by a
  Drop of kind 4, which sets its catalog to the initial one (`PROTOCOL.md` section 6.5).
- *Consequence:* the compiler's phase 8 has nothing to compare, and prefixes are allocated once per tenant. Phase 9
  still allocates through the metadata group's log, so stage 9.5 extends an allocation that already never reissues.
- The catalog encoding defines the entry kinds a stage 6 compiler emits, Describe's kinds 1 to 19. The retired-name
  and migration entries of `SCHEMA.md` section 10.2 arrive with stage 9.5, which writes them; the wire decodes
  Describe's kind 20 from stage 6, as `PROTOCOL.md` lays it out.
- *Alternative, an Install that adds sources to a governed tenant:* additive in section 13.2's terms and cheap to
  build, and it is still a version taking effect over data, which the user places whole at stage 9.5.

### 3. A UQL CRUD subset carries stage 6's data operations (user, 2026-11-10)

The preparation's D3 wrote instances as quads alone. Quads cannot name a delete or a retraction of an instance, and
Gen5's every removal is a retraction. Query therefore takes `language` 3 for this subset of
`specification/query/QUERY-ULTRA-QL.md`, added for testing:

```
request   = [ "use" qident ] ( query | mutation ) ;
query     = selector [ "limit" int ] shape ;
selector  = qident [ "(" [ fitem { "," fitem } ] ")" ] ;
fitem     = ident cmp_op value            (* an own attribute, or iri, against a literal or a parameter *)
          | ident ":" selector            (* a role of a relation type, bound by a point selector *)
          | "is" qident
          | "with" "retracted" ;
value     = literal | "$" ident | "[" value { "," value } "]" ;
shape     = "{" sfield { sfield } "}" ;
sfield    = [ ident ":" ] ident | ".." ;  (* an attribute or a built-in field, optionally under an alias *)
mutation  = "insert" qident values
          | "update" selector values
          | "delete" selector
          | "retract" selector ;
values    = "{" [ assign { "," assign } ] "}" ;
assign    = ident ":" ( value | selector | "[" selector { "," selector } "]" ) ;
```

- In `values`, the assigned name decides the reading: an attribute takes a value, a list of values for a `List` or
  `Set`, and a role takes a selector, a list of selectors for a set-valued role.
- Every request of the subset is a request of UQL with UQL's meaning, so stage 6's tests stay valid when UQL is built
  in full. A construct of UQL outside the subset is refused with `UQL0901`, the specification's own refusal for a
  construct before its stage.
- A **point selector** fixes its type's whole key, or `iri`, by equality. It compiles to the instance's IRI with no
  read: a keyed IRI is minted from the key space's name and the key (`SCHEMA.md` section 7.3). A role is assigned by
  a point selector; the commit checks that the participant exists, is live and is of the role's type.
- A relation selector binds any of its roles by point selectors, or none: a bound anchor role is a forward read,
  another bound role a reverse read as `PROTOCOL.md` section 11.11 serves one, and no bound role a scan of the type.
- A query's rows hold one column per shape field: a scalar as its term, a record, interval, list or set as one
  `rdf:JSON` literal in `SCHEMA.md` section 11.2's canonical form, an absent optional value as `ABSENT`
  (`PROTOCOL.md` section 9.1). Built-in fields are `type`, `iri`, `asserted` and `live`.
- A mutation answers one Result of no columns and no rows, flagged `LAST`. The subset has no `returning`: a keyless
  instance's handle is minted by its partition at creation, and a relation instance is found again by its role
  bindings.
- Several statements are made atomic by the protocol's interactive transaction: Begin, several Query frames, Commit.
- The RDF view's writes through quads stay in scope (`SCHEMA.md` section 11.4): the oracle's instance sets are written
  as quads, and a blank node names a keyless instance whose handle the partition mints.
- *Alternative, GQL's or SPARQL's CRUD from stage 7 pulled forward:* each needs its core's parser and its view of
  relations; UQL's mutations name types, roles and `retract` directly.

### 4. Instance delete and retraction are commit-time operations 5 and 6 (user: a wire form is added)

The frontend compiles a UQL request and relays frames to the transaction's partition; a `delete` and a `retract` need
a frame that says so. Operation (`PROTOCOL.md` section 7.3) gains:

| Kind | Operation | Args | Result in the Commit reply |
|---|---|---|---|
| 5 | Delete an instance | graph (term or `ABSENT`), the instance's IRI | `node` `ABSENT`, `value` 0, `created` 0 |
| 6 | Retract an instance | graph (term or `ABSENT`), the instance's IRI | likewise |

The instance's IRI is an entity's, a node-encoded relation instance's, or an edge-encoded instance's reifier's. Each
operation is decided by the commit step in commit order, as kinds 1 to 4 are, and its writes are added to the
transaction's write-set record. Kind 5 applies `SCHEMA.md` section 8.5: the `@on_delete` actions within the
partition, the tombstone (write-set operation 8), and `UGS1013`, `UGS1020` or code 23 where they refuse; on a
`@bitemporal` type it is a retraction (section 8.5). Kind 6 applies section 8.4: it closes `asserted`, runs
`@on_retract`, and is refused on a type that is not `@bitemporal` (`UGS1026`). An edge-encoded instance that holds no
reifier is deleted by the delete of its quad, change record 3.

- *Why an operation and not a change record:* a change record names a statement; a delete and a retraction name an
  instance and are decided against the partition's state at commit, which is what a commit-time operation is. The
  write-set record already holds the entity delete as operation 8.
- *Alternative, records 11 and 12 of section 4.6:* equal on the wire; it mixes instance-level intents into the
  statement-level record list every Write, DirectWrite, ConditionalWrite and Describe shares.
- A client may send kinds 5 and 6 itself, in an Operation frame or a ConditionalWrite's operations; stage 7's GQL
  `DELETE` and SPARQL updates compile to them.

### 5. The frontend compiles UQL; compute reads; the partition decides

- `ultragraphdb_schema_uql` is pure: text, parameters and a catalog version in, a typed plan or a `UQLnnnn`
  diagnostic out. It reuses `ultragraphdb_schema_syntax`'s lexer and literal rules, since UQL's tokens are UGS's.
- A write plan is change records and operations: an `insert` is the instance's triples of the RDF view (`SCHEMA.md`
  section 11.2) in one Write; an `update` replaces each assigned attribute's values; `delete` and `retract` are
  Operation kinds 5 and 6. The frontend sends them on the transaction's Bind stream as it relays a client's own
  frames, so the partition validates a UQL write exactly as it validates a quad write.
- A read plan is quad-pattern reads through compute, as a client's Read is routed (ComputeRead): a point selector
  reads its subject's triples; a type selector reads `?s rdf:type T`, answered from the type's prefix ranges, then
  each subject's triples. The frontend applies the remaining filter items and projects the shape. `update`, `delete`
  and `retract` select through the same plan inside their read-write transaction, whose read set records it.
- The subset's row assembly in the frontend is sized for tests. Stage 7's kernel in compute replaces it; the plan type
  is the seam.
- *Alternative, executing in compute:* the right home for the query core, and it would build a kernel interface in
  stage 6 that stage 7 designs.

### 6. Crates: a new group folder `ultragraphdb_schema/` (user, 2026-11-10)

| Crate | Path | Depends on | Provides |
|---|---|---|---|
| `ultragraphdb_error_diagnostic` | `ultragraphdb_error/` | — | `Diagnostic`: code, severity, source span, reason, fix; its rendering in `SCHEMA.md` section 10.4's form. Raised by the three crates below, so it takes a crate of its own (`AGENTS.md`) |
| `ultragraphdb_schema_syntax` | `ultragraphdb_schema/` | the diagnostic crate | Lexer, parser, the syntax tree with spans, the expression grammar; phase 1 |
| `ultragraphdb_schema_compile` | `ultragraphdb_schema/` | syntax, `ultragraphdb_core_catalog`, `ultragraphdb_core_ids` | Phases 2 to 7 and 10: the checked model, the report, the catalog version. Phase 9's prefixes are passed in by the caller |
| `ultragraphdb_schema_uql` | `ultragraphdb_schema/` | syntax, `ultragraphdb_core_catalog` | The UQL subset: parse, type against a catalog version, plan |

All four are pure, with no I/O and no async, so the metadata group, a test and a later language server call them
alike. Each is created in full by the group that builds it. The group that creates the first crate adds
`ultragraphdb_schema/*` to the workspace's members and the folder to `AGENTS.md`'s list of group folders. The
catalog's encoding stays in `ultragraphdb_core_catalog`, which the partitions and compute read without the compiler.
The UQL subset sits with the schema crates because it takes its tokens, its literals and every name from UGS; the
query core's crates and their folder are stage 7's to place.

### 7. Sources of the corpus

- The acceptance test reads `specification/schema/deepbrain-gen5.ugs` as Bazel `data` of its target, exported from
  `specification/schema/BUILD.bazel`: one copy of the file the success criterion names. This replaces the
  preparation's D4.
- The conformance corpus (`UltraGraphDB Schema Feedback.md` section E; preparation task 0.4) lives in
  `ultragraphdb_schema/ultragraphdb_schema_compile/testdata/ugs/`, with the instance sets under
  `ultragraphdb/ultragraphdb_tests/testdata/`. Its modules are minimal sources of its own. Its evolution pairs are
  written at stage 9.5.

### 8. `@index(ann)` is declared and unenforced (G2, decided 2026-11-08)

As the preparation's section 2 decides: an Install accepts `@index(ann)`, records it, reports `UGS0902` and builds
nothing; a `Vector<BF16>` write is refused with the later-stage refusal; a `Vector<F32>` value is stored as a value,
as today. The Write path's vector-index declaration in an ungoverned tenant stays refused with code 7.

### 9. `@pattern` takes the `regex` crate (user: accepted for now, 2026-11-10)

`SCHEMA.md` section 6.6 defines `@pattern` in the syntax of Rust's `regex` crate, anchored at both ends. The crate is
pure Rust and matches in time linear in its input, so a schema's pattern cannot stall a commit. It is pinned in the
workspace with default features off and `std`, `perf` and the Unicode tables the syntax needs.

- *Alternative, a hand-written matcher:* the syntax is the crate's, so a second implementation is a second dialect.

### 10. Acceptance runs on one partition; cross-partition refusals are tested below it

The roles run one partition by default in stages 6 and 7 (stage 5.5's proposal), so every acceptance scenario's
checks are local. Each rule's refusal with code 23 is a scenario of `ultragraphdb_cluster_partition`, which runs N
partitions in one process, with instances placed in two partitions.

### 11. The catalog on the commit path

A partition holds the catalog version in effect decoded once into arrays indexed by type prefix and by predicate ID,
with a type's fields, roles, checks and prefix ranges reached by index. Validation of an instance looks nothing up by
name and allocates nothing in the catalog. Group 7 measures the cost per instance and per triple against the baseline
`openspec/benches/benches-2026-11-09.md` before group 8 adds index writes.

### 12. Group A is the model in `lean/schema`

Tasks A.1 to A.5 of the preparation are the library `ApgModel`. This change keeps A.6, the catalog translation
modelled over the compiler's catalog structure, and group 9 runs the oracle into committed N-Quads files; Bazel never
runs `lake`.

### 13. The specification's gaps are closed in its files first

`notes/specification-gaps.md` lists each place the specification files were silent or disagreed, with a recommendation
derived from principles those files already hold: the seven gaps of this change's first draft and six found reading
`PROTOCOL.md` in full. The user accepted all thirteen on 2026-11-10, and they are applied to `SCHEMA.md`,
`PROTOCOL.md`, `QUERY-ULTRA-QL.md` and `SCHEMA-APG.md`; `SCHEMA.md` section 17 and `QUERY-ULTRA-QL.md` section 22 record
them. This change's capability specifications are written to them. The preparation's G2, the statements placing evolution
at stage 9.5 with the fence of a first schema, and Operation kinds 5 and 6 are applied to those files too. Group 0
still carries the corpus, the oracle's files and the rest of the UQL subset's marks.

## Risks / Trade-offs

- **[The commit path's cost]** Whole-instance validation reads the catalog and the instance's other triples at every
  commit → decision 11; group 7 measures before group 8 builds on it.
- **[Gen5's catalog size]** 165 type declarations with their attributes, roles and classification facts in one
  version → group 4 measures the encoded bytes against `catalog_bytes_max`, 16 MiB, before group 5 serves Install.
- **[Two write surfaces]** Quads and the UQL subset reach one validation → the frontend turns UQL into the same frames
  a client sends (decision 5), and group 10 runs each instance set of the corpus through both and compares the RDF
  view.
- **[The subset outlives its purpose]** A test language becomes a product surface by use → it is a subset of
  `QUERY-ULTRA-QL.md` with the specification's own stage refusal, and its executor is replaced by stage 7's kernel.
- **[One Install per tenant]** A schema under development is reinstalled into a fresh tenant until stage 9.5 →
  stated in decision 2; the client's helper installs a folder of sources in one call.
- **[The golden frames]** Group 1 changes frames stage 5.5 stabilised → every golden frame is regenerated by its
  generator and the conformance suite re-run before another group builds on the wire.
- **[Cross-partition relations in Gen5]** Most of Gen5's relations anchor at their subject, so with more than one
  partition most participant rules reach another partition and are refused with code 23 → decision 10; stage 8 lifts
  it.
- **[A Drop of kind 4 removes default-entry records]** The applied recommendation D changes what a drop of everything
  in a tenant leaves, which stage 5.5 specified as outliving every drop → the `schema-changes` and `metadata-group`
  deltas of this change carry it, and group 5 builds it with the Drop's catalog version.

## Migration Plan

Stores and catalogs written before this stage are not readable and are recreated. The groups land in the order of
`tasks.md`; the whole repository is gated by `make build` and `make test` when a wave returns, and the user commits
before the next wave starts.

## Open Questions

- Is the reifier of a declared relation's edge-encoded instance named by a handle IRI of its type, or by a skolem IRI
  as a statement's is (`notes/specification-gaps.md`, gap G)? Group 6 needs the answer.
