# Preparing stage 6.6: UGS, the data definition and its implementation

* Status: preparation, 2026-11-08, while stage 5.5's last mutation passes run (its tasks 9.6, 10.5, 11.5 and group 12).
* Input to: the stage 6.6 OpenSpec change, written once stage 5.5 closes with 12.8.

This note lays out stage 6.6 in the order it must be built, with enough detail that its change set (proposal, design,
capability specifications, tasks) can be written from it and its groups started without rediscovering what stage 5.5
left. Stage 6.6 builds UGS, the one data definition language of UltraGraphDB (`specification/schema/SCHEMA.md`), from its
source text to the catalog, the store and the write path, together with the protocol and client parts a schema needs.

## 1. Success

Stage 6.6 is done when:

1. **A UGS schema is created, read, updated and deleted** through the binary protocol by the Rust client:
   - **create:** Install of a schema's sources into a tenant answers a catalog version, its effective epoch and the
     compilation report, or code 28 with the diagnostics;
   - **read:** Describe answers the installed sources byte for byte and every catalog entry by its record kind, under a
     schema id that is stable for one version;
   - **update:** Install of a new version takes effect at its epoch under `SCHEMA.md` section 13's rules: additive and
     loosening changes online, a tightening after its validation scan, a destructive change only by deprecate and
     drop, a version not built on the current one refused (`UGS2001`);
   - **delete:** an Install with an empty source removes a schema's namespace from its tenant (G1, section 2).
2. **The provisional DeepBrain schema loads and is applied.** Gen5's `DeepBrainSchema.ugs` with its module
   `ServiceRadarExtension.ugs` installs into a tenant with no error, its report holding exactly the warnings and
   notes the corpus expects (`UGS0902` for constructs that execute in a later stage, `UGS0903` to `UGS0906` for
   placement and scatter reads), and a corpus of DeepBrain instances written as quads is accepted where
   it holds every stage 6.6 rule and refused, with the rule's `UGS1xxx` number, where it breaks one. The instances read
   back through the RDF view.
3. `make test` is green, every group's mutation pass is done, and the fuzz runs of group 12 are recorded.

## 2. First: two specification gaps that block success

Neither criterion of section 1 can be met against the specification as it stands. Both gaps are decided below, and
closed in `SCHEMA.md` and `PROTOCOL.md` as stage 6.6's first work (tasks 0.1 and 0.2), before the change set is written
and before any group that depends on them starts.

**G1. No operation deletes a namespace.** `SCHEMA.md` and `PROTOCOL.md` install and replace sources by name, and Drop
removes data (kinds 1 predicate, 2 graph, 3 a tenant's data, 4 everything in a tenant), but nothing removes a schema
or module, and with it its namespace, from a tenant while keeping the tenant. Success criterion 1's delete has no
operation to call.
- *Resolution, decided 2026-11-08:* an Install that names a source with an empty text removes that schema or module and
  its namespace. Removal is a destructive change under `SCHEMA.md` section 13.2: refused while a graph of the namespace
  holds live instances, another catalog entry references its items, or another module imports it (`UGS2009`), so data is
  dropped first by Drop of kind 2, graph by graph, and importers are removed before what they import. A removed
  namespace's type prefixes retire and are never reissued, and its names become retired names (`SCHEMA.md` section
  10.2), so no later schema reuses an identity the store has seen. One DDL path then covers create, update and delete,
  the `declare` action already gates it, and no new command or frame is needed.
- *Why not a Drop kind 5, namespace,* removing the schema with its data in one operation: it needs a new kind, its
  `drop` permission checked against `declare`, and a cascade over every graph, and it puts a DDL effect on a data
  command, which the one-DDL rule rules out.
- *Amends:* `SCHEMA.md` section 10.1 (phase 8 reads a removal), section 13.2 (the removal row) and the `UGS2009`
  text; `PROTOCOL.md` section 8.4 (an empty source text, the report's removal entry).

**G2. Gen5's `@index(ann)` is refused before stage 8.** `PROTOCOL.md` section 11.13 refuses a vector index declared
through a Write before stage 8, with code 7, and `SCHEMA.md` section 14 places `@index(ann)`, `quantization` and
`Vector<BF16>` storage at stage 8. Read together, they refuse Gen5's Install, whose `attribute Embedding =
Vector<BF16, 768> @index(ann, metric: cosine, quantization: int8)` is one line of 972, so success criterion 2 fails on
it.
- *Resolution, decided 2026-11-08:* `SCHEMA.md` section 14's general rule for later-stage constructs applies to UGS: the
  Install accepts `@index(ann)`, compiles and records it in the catalog version, reports it as declared but unenforced
  (`UGS0902`), and builds no index. A write of a `Vector<BF16>` value is refused until stage 8 with the later-stage
  refusal the frontend uses today; Gen5's `embedding` attributes are optional, so its instances write without them.
  Stage 8 then builds the index from the recorded declaration with no schema change. Section 11.13's refusal stays,
  restricted to the Write path's declarations in ungoverned tenants.
- *Why not strip the line from the vendored copy of Gen5:* the test would pass by testing a different schema, and every
  stage-8 construct a later schema declares would meet the same wall.
- *Amends:* `SCHEMA.md` section 14's `@index(ann)` row and the `UGS0902` text; `PROTOCOL.md` section 11.13's sentence
  on vector indexes.

## 3. Where stage 6.6 starts

What stage 5.5 leaves, read from its tasks and the code:

| Area | State at stage 6.6's start | What stage 6.6 changes |
|---|---|---|
| System ID (`ultragraphdb_core_ids`) | 128 bits, type prefix, sequence leased per (type, partition), bucket from tenant and key, the canonical key encoding | Type prefixes allocated per declared type in declaration order; buckets and claims from a key's key space; handles for keyless instances |
| Catalog (`ultragraphdb_core_catalog`) | Stage 5's three kinds: predicate declarations, entity types numbered in preorder, relations' roles; datatype checks | Replaced by `SCHEMA.md` section 10.2's catalog version: every entry, one canonical encoding, the schema id |
| Metadata group (`ultragraphdb_cluster_metadata`) | Catalog versions pending until their effective epoch; default entries for undeclared predicates; type changes checked by every partition; Report and the restoring version (task 9.5.8) | Holds compiled UGS versions; allocates prefixes; runs Install; generalises Report into section 13.3's validation scan |
| Frontend (`ultragraphdb_common_frontend`) | Refuses an Install with "stage 6.6"; answers Describe from the metadata group | Serves Install, namespace removal (G1) and the new Describe |
| Wire (`ultragraphdb_protocol_wire`) | Install, Query and Prepare frames as of 2026-11-05; golden frames to match | Brought to `PROTOCOL.md` as extended on 2026-11-08: warnings (section 4.9), Describe's record kinds and continuation, Install's `staged`, Reference's get-only mode, codes 30 and 31, datatypes 34 to 36, Purge (24) and Migrate (25), `catalog_bytes_max`. Every golden frame of Result, Prepare, Describe, Install and Reference is stale until then |
| Write path (`transactions_*`, `store_apply`, `cluster_partition`) | Quads checked against predicate declarations; registrations at a referenced entity's home; `CrossPartition` refusals | Instances assembled from their triples and validated whole at commit (`SCHEMA.md` section 8) |
| Client (`ultragraphdb_client_*`) | Transactions, reads, Describe, sessions, levels | Install, removal, the new Describe, warnings, the new codes and datatypes |

No crate parses UGS today. Stage 5.5's write-path declarations (kind 8 records in a Write, `PROTOCOL.md` section
11.13) stay for ungoverned tenants.

## 4. Scope: what stage 6.6 builds

`SCHEMA.md` section 14 is the cut. It replaces the 2026-11-01 table of `archive/review-stages-5.5-and-6.md` (decision D5),
which left bitemporality, geometry and units to parsing only; section 14, written after the bitemporal, measure and
geometry rules were settled, builds them in stage 6.6.

| Built and enforced in stage 6.6 | Accepted, compiled and recorded, executing later (`UGS0902`) |
|---|---|
| The whole grammar, every static rule of sections 5 to 7, prefixes, keys, IRIs, Describe | `@index(ann)`, `Vector<BF16>` storage and `quantization` (stage 8; gap G2) |
| Required, optional, defaults, value checks, datatypes, enums, `@immutable`, type fixed for life, closed by default, `@open` | A key or `@unique` claim at another partition (stage 8) |
| `@key` and `@unique` local to the transaction's partition | Every cross-partition relation rule (stage 8) |
| Relations: roles, set-valued and optional roles, unions, `@set`, `@multi`, `@symmetric`, inheritance and role specialisation, cardinality on the anchor role, `@on_delete` local, `@acyclic` within a partition, `@no_overlap` and `@limit` per the anchor role, `@retracts` local | Functions and virtual derivations executed (stage 7, the query core) |
| `@append_only`, `@bitemporal`, `asserted`, `retract`, `@on_retract` local, `@ends` | The GQL view and SHACL export (stage 7) |
| Entity delete with its tombstone; namespace removal (G1) | Purge (stage 8); migrations (stage 9.5) |
| Indexes `range`, `interval`, `geo`, `fulltext` | Write classification's segmented and coordinated paths across partitions (stage 8) |
| `Measure`, `quantity`, `unit`, `Geo` operators | |
| Materialized derivations within the anchor partition, computed ones among them | |
| The RDF view, reads and writes through quads | |

What Gen5 needs from this (`specification/schema/deepbrain-gen5.ugs`): 76 entity and 87 relation declarations in the
schema and 8 and 1 in its module, `@index` 89 times (66 `range`, 13 `fulltext`, 6 `interval`, 3 `geo`, 1 `ann`),
`@maps_to` 56 times, `@append_only` 20, `@set` 19, `@check` 9, `@symmetric` 5, `@unique` 5, `@range` 5, `@virtual` 5,
`@multi` 4, `@colocate` 3, `@limit` 3, `@on_retract` 3, `@pattern` 3, `@no_overlap` 2, and one each of `@bitemporal`,
`@acyclic`, `@key`, `@materialized`, `@iri`, `@immutable`, `@retracts`, `@ends`, `@deprecated` and `@default`; nine
imported vocabularies; one graph, `ontology`, the tenant's default; one module, `sr`, extending `deep_brain >= 5`. All
of it lies in the left column but the one `@index(ann)`, which G2 places in the right.

## 5. Further decisions

Decided 2026-11-08; the change set is written to them.

**D1. The change's capability specifications.** `SCHEMA.md`, `SCHEMA-APG.md` and `PROTOCOL.md` are the specification;
the change's specs are written from them as Gherkin-ready requirements.
- *Decided:* new capabilities `ugs-syntax`, `ugs-semantics`, `ugs-identity`, `ugs-catalog`, `ugs-enforcement`,
  `ugs-views` and `ugs-evolution`; deltas to `system-identity`, `catalog`, `schema-changes`, `schema-storage`,
  `mapping-contract`, `term-dictionary`, `metadata-group`, `wire-protocol` and `client-endpoint`, as `SCHEMA.md`
  section 15 lists them for stage 6.6.

**D2. The crates.** Parsing and compiling are new code with no home.
- *Decided:* `ultragraphdb_core_ugs_syntax` (lexer, parser, syntax tree with spans, the diagnostics' rendering)
  and `ultragraphdb_core_ugs_compile` (phases 2 to 10 of `SCHEMA.md` section 10.1, producing the catalog version
  through `ultragraphdb_core_catalog`), both under `ultragraphdb_core/`, pure, with no I/O and no async, so the
  metadata group, a test and a future language server call them alike. Each is created in full by the group that
  builds it; no skeleton.

**D3. Where stage 6.6's data comes from.** The query core is stage 7, so no GQL, SPARQL or UQL text reaches the engine
in stage 6.6.
- *Decided:* the acceptance corpus writes instances as quads through Write and DirectWrite, the RDF view's
  triples of `SCHEMA.md` section 11.2, and reads them back with Read's quad patterns, so stage 6.6 proves the RDF view
  and the write path without a query language.

**D4. The provisional schema's copy.** Gen5 lives in the scrith repository, and Bazel's targets stay hermetic.
- *Decided:* the corpus vendors `DeepBrainSchema.ugs` and `ServiceRadarExtension.ugs` unchanged under the
  acceptance crate's `testdata/ugs/gen5/`, with the scrith commit they were taken from; a fix Gen5 needs to compile
  is made in scrith first and copied again.

## 6. The groups, in order

Every group follows stage 5.5's rules (its `tasks.md` preamble): `openspec/protocols/tdd-protocol.md`'s five phases,
scenarios verbatim as Gherkin under each crate's `features/` with `fail_on_skipped`, every expected value from a source
independent of the code, an Ultra Style lift in a commit of its own before an existing crate's behaviour changes,
fuzz targets written for every decoder of client, peer or disk bytes, `scripts/mutants.sh` once per crate as the
group's last step, `make test` as the gate, and a commit by the user after each group.

### Group A. The APG model and the oracle → `lean/schema/ApgModel`, before stage 6.6

Written now, beside stage 5.5's last mutation passes: it needs `SCHEMA-APG.md` alone, shares no crate with stage 5.5,
and checks the decisions of `SCHEMA-APG.md` section 21 before the change set is written to them. As
stage 5.5's group 6b: built by `lake` outside Bazel on the toolchain of `lean/lean-toolchain`, its scenarios Lean
theorems named after them, no Gherkin and no `bdd` target, done when `lake build ApgModel` is green with no `sorry`,
the library added to `lean/lakefile.lean` beside `RoundModel` and `PlacementModel`, its sources under `lean/schema/`;
its plan is `archive/prepare-apg-lean.md`.
- **A.1** The binary core of `SCHEMA-APG.md` section 20: types, values, APGs, homomorphisms, mapping terms, `Δ`, with
  propositions 5.3, 6.3, 6.6, 7.2, 8.4, 8.7, 9.3, 9.4 and 9.5; `DecidableEq` for the nested types written by hand,
  since Lean 4.14 does not derive it.
- **A.2** The named layer with proposition 3.3 for values and terms; part 2's collections, sets as duplicate-free
  lists in a decidable order; the view `R` of definition 17.1, executable on the named layer, with proposition 17.3.
- **A.3** The oracle:
  - the APG schemas of `SCHEMA.md` section 16's aviation example and of a Gen5 subset covering each relation shape,
    written by hand as Lean definitions from `SCHEMA-APG.md` part 3's catalog translation;
  - a generator of valid APGs on a schema, which stage 7's GQL half (`P`, definition 18.1) reuses;
  - a `lake exe apg_oracle <dir>` writing, per schema, its generated instance sets and their `R(G)` as N-Quads into
    the folder it is given; group 9 runs it into the `testdata/apg/` folder of its test and commits the files with the
    Lean commit that wrote them. Bazel's tests read the files and never run `lake`, so the targets stay hermetic.
- **A.4** Each proposition the engine must keep restated as a property group 9's test checks on every committed
  instance set, proposition 17.3's round trip first, listed in the group's notes, as stage 5.5's 6b.4 did.
- **A.5** A finding against `SCHEMA-APG.md` fixed in the specification first, its section 21's decisions among it,
  before task 0.3 writes the capability specifications.
- **A.6** After group 4, as a task of stage 6.6: `SCHEMA-APG.md` part 3's catalog translation modelled over a catalog
  structure `ultragraphdb_core_ugs_compile` exports, so the step from UGS to the APG schema joins the oracle, and A.3's
  hand-written schemas checked against it.

### Group 0. Specification and corpus first

- **0.1 G1, namespace removal.** `SCHEMA.md` sections 10.1 and 13.2, the `UGS2009` text and `PROTOCOL.md` section 8.4
  amended as section 2 states; the removal's scenarios written: removal of an empty namespace, refusal while a graph
  holds live instances, refusal while a module imports it, retired prefixes never reissued, a retired name refused where
  a new declaration would reuse it.
- **0.2 G2, `@index(ann)` declared and unenforced.** `SCHEMA.md` section 14's row, the `UGS0902` text and `PROTOCOL.md`
  section 11.13 amended as section 2 states; the scenarios written: Gen5's `Embedding` declaration accepted with its
  note, no index built, a `Vector<BF16>` write refused, a Write-path vector index declaration in an ungoverned tenant
  still refused with code 7.
- **0.3** The change's capability specifications written from `SCHEMA.md` (D1), each requirement with its scenarios;
  the scenario of every diagnostic number taken from the corpus below.
- **0.4** The UGS conformance corpus, a folder of sources and expectations independent of the compiler:
  - for every diagnostic of `SCHEMA.md` section 10.4, a minimal source that raises it and nothing else, with the
    expected code, location and fix;
  - valid sources: `SCHEMA.md` section 16's aviation example, Gen5 with its module (D4), and one per construct of
    section 4's left column;
  - for the aviation example, the expected catalog version entry by entry, written by hand from `SCHEMA.md`, and the
    expected RDF view of its instance sets, which are group A's oracle files;
  - evolution pairs, version `n` and `n + 1`, for each row of `SCHEMA.md` section 13.2, the removal of G1 among them,
    with the expected kind and refusal;
  - instance sets for Gen5 that hold or break each stage 6.6 rule, with the expected `UGS1xxx` number.
  The corpus is the feedback note's proposal (`UltraGraphDB Schema Feedback.md`, section E), built now.

### Group 1. The wire to `PROTOCOL.md` → `ultragraphdb_protocol_wire`, `ultragraphdb_protocol_transport`

First, because the frontend, storage and the client all speak it, as stage 5.5's 9.5.1 and 9.5.3b came first.
- Bodies, views and builders for every change `PROTOCOL.md` received on 2026-11-08 and in task 0.1: the warnings
  structure (section 4.9) in Result's first batch, Prepare's and Install's replies; Describe's sections 7 and 8, its
  request of nine sections and its continuation frames; Install's `staged`, section 3 and an empty source text;
  Reference's `mode` and `status`; codes 30 `EntityDeleted` and 31 `Fenced`; datatypes 34 to 36 with tags `0x41`,
  `0x50` and `0x51` and their by-value payloads; Catalog's continuation and `catalog_bytes_max`; Purge (24) and
  Migrate (25), whose frames decode now and are refused by stage (section 11.2).
- The golden frames regenerated by their generators, the schema id's golden value `0x5000B62149F4DB68` checked; fuzz
  seeds for every changed body.

### Group 2. Syntax → `ultragraphdb_core_ugs_syntax` (new)

- The lexer of `SCHEMA.md` section 3: identifiers, reserved words, contextual words, literals of every kind with their
  validity (`UGS0001`–`UGS0099`), comments and `///` documentation, the bounds of section 3.7 (`UGS0013`).
- The parser of section 4's EBNF into a syntax tree with spans, recursive descent with every nesting bounded, so no
  input recurses without limit; the expression grammar shared by checks, limits, derivations and functions.
- Diagnostics rendered as section 10.4 shows them, location and fix.
- Phase 5: a structure-aware fuzz target generating sources from the grammar, seeded with the corpus, so runs reach
  the parser's depth rather than its first token; a second on raw bytes.

### Group 3. Static semantics → `ultragraphdb_core_ugs_compile` (new), needing 2

Phases 2 to 7 of `SCHEMA.md` section 10.1, each refusing with its range:
- **Resolution** (`UGS0100`–`0199`): names, qualified names, imports with version bounds, modules and what they may
  declare, reserved names, single inheritance, abstract types.
- **Typing** (`UGS0200`–`0299`): every type of section 5, defaults, enums and `@values`, records, lists and sets,
  intervals and precision, vectors, `Geo`, quantities, units and measures, named attribute types,
  catalog types as values; annotation arguments; expressions typed against their context.
- **Identity** (`UGS0300`–`0399`): keys and their trees, `@iri` templates, vocabulary bases, key spaces.
- **Relations and constraints** (`UGS0400`–`0699`): roles, union roles, specialisation with `as`, navigation names
  and cardinalities, satisfiability, mutability consistency, `UGS0601` and `UGS0602`.
- **Placement and classification** (`UGS0700`–`0799`): where every instance lives (section 9.1) and the
  classification facts of sections 9.3 to 9.6, recorded for the catalog even where their paths execute in stage 8.
- **Views** (`UGS0800`–`0899`): vocabulary IRIs and GQL names, collisions refused.
- Later-stage constructs, `@index(ann)` first, compiled and noted with `UGS0902` (G2).
- The model the phases produce is the input of group 4's encoding and of the RDF view. Done when the corpus's valid
  sources compile with exactly their expected notes, Gen5 among them, and every invalid source raises exactly its
  diagnostic.

### Group 4. The catalog version → `ultragraphdb_core_catalog`, `ultragraphdb_traits_catalog`, `ultragraphdb_cluster_metadata`, `ultragraphdb_core_ids`, needing 3

- Ultra Style lift of `core_catalog` and `traits_catalog`.
- Every entry of `SCHEMA.md` section 10.2 in its canonical encoding: entries ordered by kind and name, every field
  present, a decoder refusing every other byte sequence, so a version has one encoding and the schema id is stable;
  the retired-name and migration entries' encodings included, the first written by G1's removal, the second only by
  stage 9.5.
- Type prefixes allocated by the metadata group in declaration order (phase 9), never reissued, a removed namespace's
  retired with it; prefix sets as sorted ranges; the three-kind encoding and preorder numbering removed with their
  tests, after the user's approval.
- `catalog_bytes_max` enforced; the version carried in continuation Catalog frames.
- The catalog's loading by partitions and compute switched to the new version; stage 5's write-path declarations kept
  for ungoverned tenants.
- Phase 5: the version's decoder fuzzed, seeded with the corpus's compiled versions.

### Group 5. Install, Describe, evolution and removal → `ultragraphdb_common_frontend`, `ultragraphdb_cluster_metadata`, `ultragraphdb_core_ugs_compile`, needing 1, 3 and 4

- **Install end to end:** the frontend's gate (`declare` on the tenant), the compile of every source the tenant holds
  with those sent (phases 1 to 10), phase 8's evolution check against the version in effect (`UGS2001`–`UGS2011`),
  prefix allocation, the version committed as pending and its effective epoch; the reply with version, epoch, report
  and warnings, or code 28 with the diagnostics.
- **Evolution:** additive and loosening changes online; a tightening after section 13.3's validation scan on every
  partition holding the tenant's data, the generalisation of task 9.5.8's Report, a contradiction refusing the reply
  and restoring the previous definition; deprecate and finalize for destructive changes; a relocation refused until
  stage 8.
- **Removal** (G1): the namespace's live data, references and importers checked on every partition holding
  the tenant's data before the version without it takes effect; its prefixes retired; its names retired.
- **Describe:** the record kinds and the sources, filtered by what the principal may read, continuation frames, the
  schema id; prepared statements staled by a changed schema id (`PROTOCOL.md` section 11.7).
- **Governed tenants:** a tenant with a schema refuses undeclared data unless `@open` admits it (`UGS1001`).

### Group 6. Identity at write → `ultragraphdb_core_ids`, `ultragraphdb_core_terms`, `ultragraphdb_core_mapping`, `ultragraphdb_store_quads`, `ultragraphdb_cluster_partition`, needing 4

The transaction path is designed lean from the start (memory-lean records, terms borrowed through the write path's
reference types), as stage 5.5's groups were.
- An instance's ID takes its concrete type's prefix at creation; a keyed instance's bucket and its `@key` claim come
  from its key space and canonical key (`SCHEMA.md` section 7.2), a `@unique` claim from `Bucket::of_value`.
- Instance IRIs minted as section 7.3 states: from the key, by template, or a handle for a keyless instance, held in
  the dictionary both ways and accepted as a name (`PROTOCOL.md` section 4.4); a subject IRI that is not the one the
  key mints refused.
- `rdf:type` answered from the prefix for declared types, with no stored type quad.
- Relation encodings of section 11.1: edge or node by the roles' shape and `@key`; a reifier
  allocated when the instance has attributes, is `@multi`, or a role in the catalog may name it; `@symmetric` stored
  in ID order.
- Participants in other partitions checked by Reference's get-only mode (status and type prefix) before the commit.

### Group 7. Instances and their rules at commit → `ultragraphdb_transactions_txn`, `ultragraphdb_transactions_commit`, `ultragraphdb_store_apply`, `ultragraphdb_core_catalog`, needing 6

- **Assembly** (`SCHEMA.md` section 8.1): the triples a transaction writes assembled into instances, each validated
  whole against the catalog version in effect at the commit; an incomplete instance refused (`UGS1034`).
- **The stage 6.6 rows of section 8.2**, each refused with its number through code 7: types and closure, required and
  defaults, datatypes, enum members, units, vector dimensions, value checks, `@immutable`, keys and `@unique` local,
  their claims held through a retraction (section 7.2), participants existing, live and of the role's type, roles
  present, cardinality on the anchor role, `@set` resolving to the existing instance, `@acyclic` and `max_depth` within
  a partition, `@limit` and `@no_overlap` per the anchor role, `@append_only`, derived names refused, graphs,
  `UGS1036` and `UGS1037`.
- **Bitemporal state** (section 8.4): `asserted` as two epochs, an update as a revision of the same identity, `retract`
  closing `asserted`, `@on_retract` with `restrict open` retracting the closed instances, `@retracts`, `@ends`, the
  derived liveness of relations that are not `@bitemporal`, history kept for these types beyond the retention window.
- **Deletes** (section 8.5): `@on_delete` cascade, restrict and orphan; the tombstone, write-set operation 8, at the
  entity's home; a later write naming it refused with `EntityDeleted` (30).
- Every cross-partition case refused with `CrossPartition` (23) until stage 8, as P24 has it.
- Values: measures in their stored units, intervals with precision, `Geo` validated as GeoJSON in WGS 84, the three
  new datatypes encoded and decoded; a `Vector<BF16>` write refused until stage 8 (G2).

### Group 8. Indexes and derived state → `ultragraphdb_store_posting`, `ultragraphdb_store_apply`, `ultragraphdb_core_tok`, `ultragraphdb_traits_index`, needing 7

- `range`, `interval` (the outer extent's bounds, `SCHEMA.md` section 12.2), `geo` and `fulltext` indexes written in the
  instance's partition in its local commit; count indexes where a bound needs one.
- `@index(ann)` recorded and unbuilt (G2).
- `@materialized` derivations within the anchor partition, maintained in the same local commit (section 6.8): computed
  derivations, Gen5's `closed` among them; `@virtual` ones, Gen5's closures among them, recorded for stage 7.

### Group 9. The RDF view → `ultragraphdb_core_mapping`, `ultragraphdb_common_compute`, needing 6 to 8

- Reads (`SCHEMA.md` section 11.2): type triples for the concrete type and every supertype, attributes as literals of
  section 5.1's datatypes, a set one triple per member, intervals and precision, measures, `asserted_start`,
  `asserted_end` and `live`, node-encoded role triples, edge-encoded triples with reifiers through `rdf:reifies`,
  derived navigation properties read-only, the live slice by default.
- Writes (section 11.4): one triple at a time within a transaction, the instance checked once at commit.
- The mapping contract's RDF half checked against `SCHEMA-APG.md` section 17 by group A's oracle: every committed
  instance set written through quads, the engine's view equal to its `R(G)` file, and A.4's properties holding on
  every set.

### Group 10. The client → `ultragraphdb_client_core`, `ultragraphdb_client_tokio`, `ultragraphdb_client_compio`, needing 1 and 5

- Ultra Style lift of the three crates.
- `install(tenant, base_version, sources)` answering the version, its epoch, the report and the warnings, or the
  diagnostics; `remove(tenant, base_version, schema)` in G1's form; `describe(tenant)` answering the sources and the
  typed entries, across continuation frames; the warnings of every reply surfaced; codes 30 and 31 in the error type,
  31 retryable after a delay; the three datatypes in the client's terms; a prepared statement prepared again when the
  schema id changes.
- A helper loading a folder of `.ugs` files into one Install, so a schema in files is one call.

### Group 11. Acceptance → `ultragraphdb/ultragraphdb_tests`, needing A and 1 to 10

The success of section 1, end to end through the three role binaries and the client, over TLS and clear text:
- create, read, update and delete the aviation schema, each evolution pair of the corpus taking its expected path, the
  delete removing the namespace after its graphs are dropped and refused before;
- Gen5 and its module installed into a fresh tenant: no error, exactly the report the corpus expects, the `ann` index's
  `UGS0902` among it, Describe's sources equal to the files, the schema id stable across a restart of every role;
- Gen5's instance sets written as quads: the holding ones committed, each breaking one refused with its number, the
  committed ones read back through the RDF view equal to the corpus's expectation;
- a retraction with `restrict open` and an append-only provenance link behaving as `SCHEMA.md` section 8.4 states;
- a retracted node's `id` refused to a new node (`UGS1008`), and the retracted node still read by its IRI under `with
  retracted`;
- `sr` removed before `deep_brain`, and `deep_brain` refused while `sr` imports it;
- two tenants with different schemas isolated from each other.

### Group 12. Final verification and documentation

As stage 5.5's group 12:
- the last mutation pass;
- `scripts/fuzz.sh` over every target the change added;
- Ultra Style conformance;
- coverage of every added line;
- measurements:
  - Gen5's compile time, and its catalog version's bytes against `catalog_bytes_max`;
  - Install's latency to the effective epoch;
  - the commit-time cost of whole-instance validation per instance and per triple, against stage 5.5's commit figures;
- `AGENTS.md`, `ROADMAP.md`'s row and the backlog closed with what the change built;
- `make test` green with the final counts.

## 7. Order and parallelism

| Wave | Groups | Needs |
|---|---|---|
| 0 | A's tasks A.1 to A.5, during stage 5.5's last passes | `SCHEMA-APG.md` alone; done before task 0.3 |
| 1 | 0, its tasks 0.1 and 0.2 first; then 1 and 2 in parallel | 0.1 and 0.2 before anything else; 0 before any code |
| 2 | 3 | 2 |
| 3 | 4 | 3 |
| 4 | 5, 6 and A.6 in parallel | 5: 1, 3, 4; 6: 4; A.6: 3 and 4 |
| 5 | 7, then 8 | 6 |
| 6 | 9 and 10 in parallel | 9: A and 6 to 8; 10: 1 and 5 |
| 7 | 11, then 12 | everything |

A wave's agents work on disjoint crates, the whole repository is gated (`make build`, `make test`) when a wave
returns, and the user commits before the next wave starts.

## 8. Risks

- **The commit path's cost.** Whole-instance validation reads the catalog and the instance's other triples on every
  commit. Group 7 measures it against stage 5.5's figures before group 8 adds index writes, and the design keeps the
  catalog version's lookups allocation-free.
- **Gen5's catalog size.** 173 type declarations, their attributes, roles and classification facts in one version;
  group 4 measures the encoded bytes early, so `catalog_bytes_max`'s default is checked against a real schema.
- **Cross-partition relations.** Gen5's relations anchor at their subject, so most participant checks reach another
  partition. Reference's get-only mode carries them in stage 6.6; the rules that must read another partition's state
  wait for stage 8 and are refused with code 23, which the acceptance corpus places its instances to avoid where a
  rule is meant to pass.
- **Removal across partitions.** G1's check for live instances must see every partition holding the tenant's data,
  as the tightening scan does; group 5 builds both on the one generalised Report, so the check is written once.
- **The golden frames.** Group 1 changes frames stage 5.5 just stabilised; it regenerates every golden frame by its
  generator and re-runs the conformance suite before any other group builds on the wire.
