Every task group follows `openspec/protocols/tdd-protocol.md`'s five phases, in order, as stage 5.5's groups did:
(1) the group's public surface compiling with bare `unimplemented!()` bodies; (2) the full suite written against it
and observed failing for that reason, its run and test count recorded in `notes/group-N.md`: the group's scenarios
verbatim as Gherkin in the owning crate's `features/<capability>/`, bound by its `bdd` target with `fail_on_skipped`,
unit tests for the corner cases enumerated in advance, every error variant constructed through the public API, and
every expected value from a source independent of the code, the conformance corpus first; (3) the suite's inputs
reviewed against the corner-case enumeration; (4) the implementation, judged by the suite alone; (5) as the group's
last step, `scripts/mutants.sh <crate> --diff <base>` once per crate the group changed, and for a group decoding bytes
or text from a client, a peer or the disk, its fuzz targets and seeds written and built, the runs of
`scripts/fuzz.sh` happening once, in group 13. Before an existing crate's behaviour changes, its group lifts it into
`openspec/protocols/ULTRA_STYLE.md` in a commit of its own; every crate this change creates conforms from its first
commit and is created in full by the group that builds it, with its row in `AGENTS.md`'s crate table. Tests return
`TestResult`. Bazel is the gate: a group is done when `make test` passes, cached; the user commits after each group.
Removing files waits for the user's approval at the task that removes them. A weakness found in a specification file
is fixed in that file first.

Waves: 0; then 1 and 2 in parallel; then 2b and 3 in parallel; 4; 5 and 6 in parallel; 7; 8; 9; 10 and 11 in parallel; 12; 13. A wave's agents
work on disjoint crates, and the whole repository is gated by `make build` and `make test` when a wave returns.

## 0. Specification and corpus first

- [x] 0.1 The number refusing a revision resolved in `SCHEMA.md` sections 8.2, 8.4, 8.7 and 17 (2026-11-10): `UGS1007`, `UGS1012` and `UGS1013` on a revision as on any update, `UGS1025` for a required role's participant
- [x] 0.2 The recommendations of `notes/specification-gaps.md` presented to the user: all thirteen accepted on 2026-11-10
- [x] 0.3 G2 in the specification: `SCHEMA.md` section 14's `@index(ann)` row and the `UGS0902` text, and `PROTOCOL.md` section 11.13's sentence on vector indexes, amended as `openspec/notes/prepare-stage-6.md` section 2 decides; section 14 states the `UGS0902` of functions and virtual derivations
- [x] 0.4 Evolution at stage 9.5: `SCHEMA.md` sections 10.1, 13 and 14 and `PROTOCOL.md` sections 8.4 and 11.2 state that a tenant takes one Install until stage 9.5, with the refusals of ugs-catalog; `ROADMAP.md`'s stage 9.5 row names the changes in place and namespace removal beside the migrations
- [x] 0.5 The UQL CRUD subset, whose grammar `QUERY-ULTRA-QL.md` section 4.1 and whose `language` 3 `PROTOCOL.md` sections 7.4, 8.1, 9.1 and 11.2 hold since task 0.7: section 12 of `QUERY-ULTRA-QL.md` marks the subset's mutations **[stage 6.6]**, and `ROADMAP.md`'s sentence on UltraQL names the subset
- [x] 0.6 Instance operations: `PROTOCOL.md` sections 6.3, 6.7, 7.3, 11.6 and 12.2 take Operation kinds 5 and 6 with their results and write-set effects
- [x] 0.7 The accepted recommendations carried into `SCHEMA.md`, `PROTOCOL.md`, `QUERY-ULTRA-QL.md` and `SCHEMA-APG.md`, each at the sections its entry in `notes/specification-gaps.md` names (2026-11-10)
- [x] 0.8 `specification/schema/BUILD.bazel` exporting the `.ugs` files as test data; the group folder `ultragraphdb_schema/` added to the workspace's members and to `AGENTS.md`'s list of group folders by the group creating its first crate
- [x] 0.9 The conformance corpus under `specification/conformance/ugs/` in the layout of its `README.md`, written independently of the compiler: one minimal source per diagnostic of `SCHEMA.md` section 10.4, and `UGS2005`, with its expected code, location and fix and a twin applying the fix; the valid sources (aviation, Gen5, modules of the corpus's own, one per construct of section 14's stage 6.6 rows) with their expected warnings and the report's records of `SCHEMA.md` section 9.5; the aviation catalog version entry by entry, written by hand (2026-11-10: 119 diagnostics with 329 cases and their twins, 74 valid sources with their reports, Gen5's 1,745 UGS0901 records among them, and `catalog/aviation.json`; the readings its authors found missing written into `SCHEMA.md`, `CATALOG.md`, `QUERY-SPARQL-CORE.md`, `PROTOCOL.md` and `SCHEMA-APG.md`, `SCHEMA.md` section 17's "Corpus readings")
- [x] 0.10 The instance corpus under `ultragraphdb/ultragraphdb_tests/testdata/instances/` in the layout of its `README.md`: for Gen5 and aviation, instance sets that hold or break each stage 6.6 rule with the expected `UGS1xxx` number, each as quads and, where the subset expresses it, as UQL; a revision changing a `@limit` condition's truth and a walk past `rule_read_max` (`UGS1040`) among them (2026-11-10: 163 cases, 36 of section 8.7's refusals reached; `UGS1037` waits for stage 9.5's birth IRIs, and `UGS1039` takes a schema with arithmetic, which neither has, so group 7 declares its own)
- [x] 0.11 `lake build ApgModel` green with no `sorry`; the oracle's instance sets and `R(G)` files for aviation and Gen5 exported by `lake exe apg_oracle` and committed under `ultragraphdb/ultragraphdb_tests/testdata/apg/` with the Lean commit that wrote them (2026-11-10, `d1aad303`, four sets per schema; each set exported as its view `<n>.nq`, its write `<n>.write.nq` and, where it retracts, `<n>.retract`, after the oracle's Gen5 was brought in line with `deepbrain-gen5.ugs` (its graph, Teloid's required attributes, a cascaded retraction, an interval bound); run as `lake env lean --run schema/ApgModel/Oracle/Main.lean <dir>`, the same `main` interpreted, since an executable the 4.14.0 toolchain links fails to load on macOS 27: `__DATA_CONST segment missing SG_READ_ONLY flag`)
- [x] 0.12 `specification/schema/CATALOG.md` written before group 4 starts, by the six rules of `SCHEMA.md` section 10.2: the version's fixed part, sections and entry table, each entry kind's fixed part and sections, the prefix index, the postfix programs with their instructions and the decoder's verification, the one-encoding rules, and one example encoded by hand; `SCHEMA.md` section 10.2 and `PROTOCOL.md` sections 8.3 and 13.7 name the file
- [x] 0.13 The review's resolutions R2 to R7 carried into `SCHEMA.md`, `PROTOCOL.md`, `QUERY-ULTRA-QL.md` and `SECURITY.md` (2026-11-10; `notes/review-resolutions.md`)
- [x] 0.14 The reviewers' decisions D1 to D19 carried into `SCHEMA.md`, `PROTOCOL.md`, `QUERY-ULTRA-QL.md` and `QUERY-SPARQL-CORE.md`, and the corrections of `notes/review-resolutions.md` part 2.2 into this change's specifications (2026-11-10)
- [x] 0.15 The core principles' review, P1 to P11, carried into `SCHEMA.md`, `CATALOG.md`, `PROTOCOL.md`, this change's specifications, design and tasks (2026-11-10; `notes/core-principles-review.md`, design decision 15)

## 1. The wire → `ultragraphdb_core_body` (new), `ultragraphdb_protocol_wire`, `ultragraphdb_protocol_transport`

- [x] 1.1 Scenarios: wire-protocol's four requirements of this change, as Gherkin; the corner cases enumerated per changed body
- [x] 1.2 Phases 1 to 3: the body layout's reader and the string list's reader and writer moved from `ultragraphdb_protocol_wire` into `ultragraphdb_core_body` under `ultragraphdb_core/`, created in full with its tests, fuzz seeds and its row in `AGENTS.md`, the wire reading through it; bodies, views and builders for the warnings structure, Describe's entry kinds 1 to 20, request and continuation frames, Install's `staged` and sections, Reference's `mode` and `status`, codes 30 and 31, datatypes 34 to 36 with their tags and payloads, Catalog's continuation, `catalog_bytes_max`, Purge, Migrate, Query's and Prepare's `language` 3, Operation kinds 5 and 6 in Operation and in ConditionalWrite, Drop's `catalog`, the reifier column's handle IRIs
- [x] 1.3 Phase 4: the implementation; every golden frame of Result, Prepare, Describe, Install and Reference regenerated by its generator, the schema id's golden value `0x5FC0D2C82332781C` checked; the conformance suite green
- [x] 1.4 Phase 5: fuzz seeds for every changed body; `scripts/mutants.sh` over the three crates (2026-11-10: over the six crates the group changed and the three the Result reader's new variants reached; no survivor left, the coordinator's four decisions applied, `notes/group-1.md`)

## 2. Syntax → `ultragraphdb_error_diagnostic`, `ultragraphdb_schema/ultragraphdb_schema_syntax` (new)

- [ ] 2.1 Scenarios: ugs-syntax, as Gherkin; the corpus's syntax sources bound as a table-driven test
- [ ] 2.2 Phases 1 to 3: `Diagnostic` with code, severity, span, reason and fix and its rendering; the token and syntax-tree types borrowing spans of the source, copying no name or literal; the lexer's and parser's entry points; the corner cases of every literal kind, every bound of section 3.7 at and one past its limit, every contextual keyword in a name position
- [ ] 2.3 Phase 4: the lexer of `SCHEMA.md` section 3, `statement` a keyword after `abstract` alone, and the parser of section 4 with no recursion (Ultra rule 1): declarations in a loop over an explicit stack of at most 64 levels, an expression in an operator-precedence loop emitting the postfix form of `CATALOG.md` section 4, shared by checks, limits, derivations and functions; a mistyped keyword's case named as its fix
- [ ] 2.4 Phase 5: a structure-aware fuzz target generating sources from the grammar, seeded with the corpus, and one on raw bytes; `scripts/mutants.sh` over both crates

## 2b. The commit path in Ultra Style → `ultragraphdb_transactions_txn`, `ultragraphdb_transactions_commit`, `ultragraphdb_store_apply`, `ultragraphdb_store_quads`, `ultragraphdb_cluster_partition`

The lifts groups 6 to 8 build on, as a group of their own beside group 3, which touches none of these crates
(`notes/core-principles-review.md`, P10). Behaviour does not change.

- [ ] 2b.1 Each crate lifted into `openspec/protocols/ULTRA_STYLE.md`, each in its own commit: its planes stated, every loop and queue bounded, no recursion, a reason for each `Arc`
- [ ] 2b.2 Data-plane memory sized at startup: the transaction buffers (`txn_buffers`), the changeset's maps and sets reused across transactions, quad keys and dictionary keys on fixed buffers
- [ ] 2b.3 The commit queue bounded by a named setting that every production caller sets, a full queue answering code 5
- [ ] 2b.4 The catalog held as design decision 11 has it: shared read-only by every execution, the per-execution clone and the five other copies gone
- [ ] 2b.5 Done when the suites pass unchanged and `make test` is green; throughput compared with `openspec/benches/benches-2026-11-09.md` in a dated file under `openspec/benches/`, each change kept on evidence
- [ ] 2b.6 Phase 5: `scripts/mutants.sh` over each lifted crate

## 3. Static semantics → `ultragraphdb_schema/ultragraphdb_schema_compile` (new)

- [ ] 3.1 Scenarios: ugs-semantics and the compile-time scenarios of ugs-identity (keys, templates, vocabulary IRIs), as Gherkin; the corpus bound as table-driven tests, one per diagnostic range
- [ ] 3.2 Phases 1 to 3: the checked model's types and the compiler's entry point taking sources and prefixes; the corner cases per phase
- [ ] 3.3 Phase 4, resolution and typing (`UGS0100` to `UGS0299`), every walk a bounded worklist: names, imports, modules, inheritance, name spaces of kinds and units; every type of section 5, defaults, annotations' arguments, expressions typed against their context
- [ ] 3.4 Phase 4, identity and relations (`UGS0300` to `UGS0699`): keys, key spaces, `@iri` templates, bases; roles, unions, specialization, navigation names, cardinalities, the constraint rules, mutability consistency
- [ ] 3.5 Phase 4, placement, classification and views (`UGS0700` to `UGS0899`): anchors, the classification of every write operation, the request-typing facts, vocabulary IRIs, collisions; the report's records `UGS0901` to `UGS0906` and their rendering, later-stage constructs under `UGS0902`; checks, limits and computed derivations as the parser's postfix programs, typed with a type stack and verified, path derivations compiled to their steps, function bodies stored as their text, with `UGS0228` and `UGS0807`
- [ ] 3.6 Done when every valid source of the corpus compiles with exactly its expected notes, Gen5 among them, and every invalid source raises exactly its diagnostic; Gen5's compile time recorded
- [ ] 3.7 Phase 5: `scripts/mutants.sh ultragraphdb_schema/ultragraphdb_schema_compile`

## 4. The catalog version → `ultragraphdb_core_catalog`, `ultragraphdb_traits_catalog`, `ultragraphdb_cluster_metadata`, `ultragraphdb_core_ids`

- [ ] 4.1 Ultra Style lift of `core_catalog` and `traits_catalog`, each in its own commit
- [ ] 4.2 Scenarios: ugs-catalog ("A catalog version holds the compiled schema", "A catalog version has one encoding", "The schema id identifies what a principal reads of a version", "A catalog version is bounded"), ugs-identity's prefix requirements, catalog's and metadata-group's deltas, system-identity's delta
- [ ] 4.3 Phases 1 to 3, from `CATALOG.md` (task 0.12): the entry types, the canonical encoder, the verifying decoder with the checksum, the prefix index and the programs' verification, the predicate index, a path derivation's steps, the views of design decision 11, prefix and predicate-sequence allocation in the metadata group's state machine, `Resource` leases held below 2^63; the hand-encoded example of `CATALOG.md` as the layout's golden bytes, and the aviation and Gen5 versions checked entry by entry and field by field against the corpus through the decoder, their bytes pinned once those checks pass
- [ ] 4.4 Phase 4: the encoding, the emission of phase 10 in `ultragraphdb_schema/ultragraphdb_schema_compile` with its decode and re-encoding, prefix and predicate-sequence allocation committed with its version, Gen5's encoded bytes measured as soon as it is emitted against the 523,000-byte cap of today's code and one Catalog frame, prefix sets as sorted ranges, the canonical key encoding and key-space buckets in `core_ids`
- [ ] 4.5 The three-kind encoding and the preorder numbering removed with their tests, after the user's approval; the write-path declarations kept for tenants with no schema
- [ ] 4.6 Catalog loading by partitions, compute and the frontend switched to the new version, carried in continuation Catalog frames; one immutable buffer per version per process, shared by the metadata group's head, state and snapshots and written as slices into replies; each endpoint core and partition holding the version in effect per tenant, refreshed at a close; `catalog_bytes_max` enforced, and the retained bytes per tenant within 16 times it
- [ ] 4.7 Phase 5: the version's decoder fuzzed, seeded with the corpus's compiled versions; `scripts/mutants.sh` over the four crates

## 5. Install and Describe → `ultragraphdb_common_frontend`, `ultragraphdb_cluster_metadata`

- [ ] 5.1 Scenarios: ugs-catalog ("Install compiles a tenant's sources into a new version", "A tenant takes one Install until stage 9.5", "Describe answers the sources and the entries", "A tenant with a schema is governed", "Every role reads the version in effect"), client-endpoint's "The frontend serves Install" and its two modified requirements, metadata-group's "The metadata group holds compiled catalog versions with their sources" and its modified requirement, schema-changes' "Drops"
- [ ] 5.2 Phases 1 to 3: the frontend's Install handler; its compile thread, started at startup behind a bounded queue, compiling and verifying off the connection's and the owner's turns; the owner's commit of a version's bytes with its sources, prefixes and predicate sequences while the base and both next free values are still the tenant's; Describe answered by the endpoint core from its version's bytes, with continuation
- [ ] 5.3 Phase 4: Install end to end, with code 28 and its diagnostics, code 2 for a stale base, code 7 for a tenant that holds a schema, a migration source, or ungoverned data, and for a compiled version failing its decode or re-encoding, injected through the test utilities; code 5 for a full compile queue; epochs closing while a compile is held; the reply with version, epoch, report and warnings; a Drop of kind 3 or 4 naming its catalog version, refused when stale, and a Drop of kind 4 returning the tenant to the initial catalog with its default-entry records removed
- [ ] 5.4 Phase 4: Describe from the metadata group with the sources byte for byte, continuing past one frame where the frontend's scenario "A Describe past the client's frame is refused" stood, refused past `catalog_bytes_max` with code 6, the schema id, prepared statements staled by a changed schema id
- [ ] 5.5 Phase 4: the governed tenant at the frontend and the partition: undeclared predicates and graphs refused, `@open` recording default entries, write-path declarations and a declared predicate's Drop refused, one-frame writes held to blind predicates with instance-local rules validated in the frame's turn, operations 1 and 2 refused; a transaction spanning a version aborting with Conflict of kind Catalog
- [ ] 5.6 Phase 5: `scripts/mutants.sh` over both crates; Install's latency to the effective epoch measured for Gen5
- [ ] 5.7 Isolation between tenants, as scenarios of `ultragraphdb_cluster_metadata` and `ultragraphdb_cluster_partition`, where the group's own command makes a second tenant: two tenants holding different schemas, each refusing the other's types and describing its own sources alone

## 6. Identity at write → `ultragraphdb_core_ids`, `ultragraphdb_core_terms`, `ultragraphdb_core_mapping`, `ultragraphdb_store_quads`, `ultragraphdb_cluster_partition`

- [ ] 6.1 Ultra Style lift of each existing crate the group changes that group 2b did not lift, each in its own commit
- [ ] 6.2 Scenarios: ugs-identity's write-time requirements (buckets and claims, keyed IRIs, handles, routing), term-dictionary's delta, mapping-contract's delta, ugs-views' "A relation type is edge-encoded or node-encoded by its shape"
- [ ] 6.3 Phases 1 to 3: the instance's ID from its concrete type's prefix; IRI minting from key, template and handle; the frontend's and the partition's routing of a keyed and a handle IRI; golden buckets from the xxHash reference
- [ ] 6.4 Phase 4: IDs, buckets and `@key` and `@unique` claims; handle IRIs minted by the creating partition and held both ways, a reifier's among them; a transaction creating a node-encoded relation instance under a blank node bound to its anchor's partition; a recreated IRI's entry rewritten, its record naming the ID it replaced; a subject IRI its type does not mint refused; `rdf:type` answered from the prefix with no stored quad; in a governed tenant, declared predicates resolved from the version in effect by the partitions and compute, with no dictionary entry, lock, store read or registration
- [ ] 6.5 Phase 4: relation encodings: edge or node by the roles' shape and `@key`, the reifier's allocation rule, `@symmetric` in ID order, the `@set` combination key in the marks keyspace; participants in other partitions answered by Reference's get-only mode
- [ ] 6.6 Phase 5: `scripts/mutants.sh` over each changed crate

## 7. Instances and their rules at commit → `ultragraphdb_transactions_txn`, `ultragraphdb_transactions_commit`, `ultragraphdb_store_apply`, `ultragraphdb_core_catalog`, `ultragraphdb_protocol_wire`

- [ ] 7.1 Each crate the group changes checked against `ULTRA_STYLE.md` as groups 2b and 4 left it, a gap lifted first in its own commit
- [ ] 7.2 Scenarios: ugs-enforcement but its two index and derivation requirements; each cross-partition refusal as a scenario of `ultragraphdb_cluster_partition` with two partitions
- [ ] 7.3 Phases 1 to 3: the assembly of a transaction's changes into instances, a creation being an `rdf:type` on a blank node or a minted IRI; the validator over the decoded catalog version, reporting the first rule broken in `SCHEMA.md` section 8.2's order; Operation kinds 5 and 6 in the commit step; the corner cases per row of `SCHEMA.md` section 8.2
- [ ] 7.4 Phase 4, assembly and values: whole-instance validation, incomplete instances, concrete types, required attributes and defaults, datatypes, enums, units and measures, intervals with precision, `Geo`, vector dimensions, value checks evaluated as `SCHEMA.md` section 4 states, two-valued, vacuous over an absent attribute value and failing closed with `UGS1039`, their programs run without recursion or allocation, with `@pattern` on `regex-automata`'s `meta::Regex` in the `regex` syntax, each partition building its patterns and one `Cache` per pattern at load, pinned in the workspace with their rows in `AGENTS.md` as design decision 9 states
- [ ] 7.5 Phase 4, keys and relations: keys, `@unique`, `@immutable`, permanent claims of history-keeping types, participants, roles, a required role's participant fixed, cardinality on the anchor role, `@set`, `@multi`, `@symmetric`, `@acyclic` and `max_depth` within a partition, `@limit` with its aggregate per group in the marks keyspace written in the commit that moves it, an update changing a condition's truth among those, and `@no_overlap`; `@acyclic` and `@no_overlap` within `rule_read_max` and `UGS1040` past it
- [ ] 7.6 Phase 4, mutability and history: `@append_only`, `asserted` as two epochs, revisions refused under their rules' own numbers, retraction through Operation kind 6, `@on_retract` with `restrict open`, `@retracts`, `@ends`, the derived liveness of relations that are not `@bitemporal`, history kept beyond the retention window
- [ ] 7.7 Phase 4, deletes: Operation kind 5, `@on_delete` cascade, restrict and orphan, the tombstone as write-set operation 8, the wire's write-set operations 8 and 9 (`PROTOCOL.md` section 12.2) with their replay, `EntityDeleted`; every cross-partition case refused with code 23
- [ ] 7.8 The commit-time cost of validation per instance and per triple measured against `openspec/benches/benches-2026-11-09.md`, recorded in a dated file under `openspec/benches/`, before group 8 starts
- [ ] 7.9 Phase 5: fuzz seeds for the write-set records the group changed; `scripts/mutants.sh` over each changed crate

## 8. Indexes and derived state → `ultragraphdb_store_posting`, `ultragraphdb_store_apply`, `ultragraphdb_core_tok`, `ultragraphdb_core_types`

- [ ] 8.1 Ultra Style lift of each existing crate the group changes that group 2b did not lift, each in its own commit
- [ ] 8.2 Scenarios: ugs-enforcement's "Indexes are written in the instance's local commit" and "Materialized derivations are maintained with the write", tokenizers' and scalar-types' deltas
- [ ] 8.3 Phases 1 to 3 and 4: the three datatypes; the `measure` tokenizer; `range` with the `decimal` tokenizer for `Decimal`, `interval` over the outer extent, `geo` with a line string's cover added to the geo tokenizer, and `fulltext` indexes and count indexes in the instance's local commit; `@index(ann)` recorded and unbuilt
- [ ] 8.4 Phase 4: materialized derivations within the anchor partition, computed ones first, Gen5's `closed` with its range index; the path evaluator for materialized paths, a loop over the compiled steps with an explicit frontier and a visited set, no recursion and no allocation after load, recomputing each instance its changes reach by the backward walk, `UGS1040` past `rule_read_max`; virtual derivations recorded
- [ ] 8.5 Phase 5: `scripts/mutants.sh` over each changed crate

## 9. The RDF view and the oracle → `ultragraphdb_core_mapping`, `ultragraphdb_common_compute`, `lean/schema`

- [ ] 9.1 Scenarios: ugs-views but its encoding requirement, mapping-contract's "A governed tenant's RDF view is the APG's"
- [ ] 9.2 Phases 1 to 4: reads of definition 17.1: type triples from the prefix ranges, attributes, sets, lists and records as canonical `rdf:JSON`, intervals, measures, `asserted` and `live`, role triples, reifiers through `rdf:reifies`, the live slice by default and versions at an earlier epoch
- [ ] 9.3 Phase 4: writes through quads: a blank node given its handle, one role triple at a time, derived properties refused
- [ ] 9.4 The oracle test: every committed instance set of task 0.11 written as its `<n>.write.nq` through quads, then its `<n>.retract` IRIs retracted by Operation kind 6 in one transaction, the engine's view equal to its `R(G)` file `<n>.nq`, the `ugdb:asserted_*` values left out of the comparison since the engine records its own commit times, and each property of the model's notes holding on every set
- [ ] 9.5 The catalog translation of `SCHEMA-APG.md` part 3 modelled in Lean over a catalog structure `ultragraphdb_schema/ultragraphdb_schema_compile` exports, and the oracle's hand-written aviation and Gen5 schemas checked against it (the preparation's A.6)
- [ ] 9.6 Phase 5: `scripts/mutants.sh` over both crates

## 10. The UQL CRUD subset → `ultragraphdb_schema/ultragraphdb_schema_uql` (new), `ultragraphdb_common_frontend`

- [ ] 10.1 Scenarios: uql-crud, and client-endpoint's "The frontend compiles a UQL request and sends it as frames"
- [ ] 10.2 Phases 1 to 3: the plan types, written into buffers the connection holds from its start, sized by `max_frame` and the nesting bound, cleared and reused per request; a write plan of change records and operations and a read plan of pattern reads; the compiler's entry point taking text, parameters and a catalog version; one refused request per `UQL` diagnostic the subset raises; a table of UQL constructs outside the subset, each expecting `UQL0901`
- [ ] 10.3 Phase 4: the parser over `ultragraphdb_schema/ultragraphdb_schema_syntax`'s lexer, with no recursion, as task 2.3's; typing against the catalog; point selectors compiled to IRIs; plans for insert, update, delete, retract and the root query, forward, reverse and by type
- [ ] 10.4 Phase 4: the frontend's Query handler for `language` 3: a write plan sent on the transaction's Bind stream as a client's frames are relayed, a read plan routed to compute, rows filtered and projected, `limit`, the reply's columns and `rdf:JSON` cells
- [ ] 10.5 The parity test: each instance set of task 0.10 that the subset expresses written through UQL and through quads, the two RDF views equal and the refusals alike
- [ ] 10.6 Phase 5: a structure-aware fuzz target for the subset's parser seeded with the corpus's requests, and one on raw bytes; `scripts/mutants.sh` over both crates

## 11. The client → `ultragraphdb_client_core`, `ultragraphdb_client_tokio`, `ultragraphdb_client_compio`

- [ ] 11.1 Ultra Style lift of the three crates, each in its own commit
- [ ] 11.2 Scenarios, as the client's own Gherkin: an Install answered with version, epoch, report and warnings; an Install refused with its diagnostics; Describe across continuation frames; a UQL query's rows; a UQL refusal with its diagnostic; a delete and a retraction sent as operations; codes 30 and 31, 31 retryable after a delay; the three datatypes as terms
- [ ] 11.3 Phases 1 to 4: `install(tenant, base_version, sources)`, `describe(tenant)`, `query(transaction, text, parameters)` for `language` 3, `delete_instance` and `retract_instance` as Operation kinds 5 and 6, a Drop naming its catalog version, the warnings of every reply surfaced, the new codes in the error type, the new datatypes in the client's terms, a prepared statement prepared again when the schema id changes
- [ ] 11.4 A helper loading a folder of `.ugs` files into one Install
- [ ] 11.5 Phase 5: `scripts/mutants.sh` over the three crates

## 12. Acceptance → `ultragraphdb/ultragraphdb_tests`

- [ ] 12.1 Gen5 loads and is applied: `deepbrain-gen5.ugs` installed into tenant 0 of a fresh cluster, each battery starting its own, through the three role binaries and the Rust client, over TLS and clear text: no error, exactly the report the corpus expects, Describe's source equal to the file, the schema id equal after a restart of every role
- [ ] 12.2 Simple CRUD over Gen5 through the UQL subset: a `Company` and a `Person` inserted and read back with their defaults; a `MemberOf` inserted and found by its anchor and by its member; a `Post` with its `PostIn` in one transaction; an update read back as a revision under the same IRI; a retraction leaving the default read and kept under `with retracted`; the retracted `id` refused to a new node (`UGS1008`)
- [ ] 12.3 Gen5's rules at commit: each rule-breaking instance set of task 0.10 refused with its number, each holding set committed and read back through the RDF view equal to the corpus's expectation; a retraction under `restrict open` and an append-only provenance link behaving as `SCHEMA.md` section 8.4 states
- [ ] 12.4 The loop of this stage: Gen5's tenant emptied by a Drop of kind 4 naming its catalog version, a Drop naming a stale version refused, the aviation schema installed in its place and nothing of Gen5 read; then aviation's mutable entities and relations inserted, updated and deleted through the UQL subset, a delete leaving its tombstone, a restricted delete refused
- [ ] 12.5 A second Install into the tenant refused naming stage 9.5; a write between an Install's reply and its effective epoch refused with code 31 and committed on its retry; a fresh cluster's tenant with no schema behaving as before this stage
- [ ] 12.6 Every acceptance test prints evidence that it ran against the built binaries

## 13. Final verification and documentation

- [ ] 13.1 The last mutation pass: survivors listed in the groups' notes killed by a test or recorded with their measurement
- [ ] 13.2 `scripts/fuzz.sh` over every target the change added; each finding minimized, committed under `testdata/fuzz_regressions/` and replayed by a unit test
- [ ] 13.3 Ultra Style conformance of every crate the change touched; coverage of every added line by `scripts/coverage.sh`
- [ ] 13.4 Measurements recorded in a dated file under `openspec/benches/`: Gen5's compile time and its catalog version's bytes against `catalog_bytes_max`; Install's latency to the effective epoch; the commit-time cost of whole-instance validation against the 2026-11-09 baseline
- [ ] 13.5 `AGENTS.md`'s crate table, `ROADMAP.md`'s stage 6.6 row and `openspec/notes/backlog.md` closed with what the change built; `SCHEMA.md` section 14's stage column checked against it
- [ ] 13.6 `make test` green with the final counts recorded in `notes/group-13.md`
