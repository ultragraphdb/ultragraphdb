# UltraGraphDB Schema Feedback: DeepBrain Gen5 written in UGS

* Status: findings of a non-binding exercise, 2026-11-05.
* Method: `scrith/openspec/notes/schema/gen5/schema_graph_gen5.md`, DeepBrain's Gen5 draft written in the UGS of the
  design note, was rewritten against `specification/SCHEMA.md` as it stands. The result is
  `scrith/openspec/notes/schema/gen5/DeepBrainSchema.ugs` (the core, 77 entity types, 69 relation kinds, 18 links,
  31 enums, 11 quantity kinds, 22 units, 5 derivations, 10 functions) and `ServiceRadarExtension.ugs` (the module).
  Every place the draft could not be written as it stood carries a marker `[E<n>]` in the files; the numbers below are
  those markers. The purpose: whether UGS expresses a Gen5 schema whole (A), where its rough edges are (B), and which
  gaps and design flaws the application exposed (C), to inform the next iteration of the specification.

## A. What UGS expresses whole

Every construct the gaps note (`ALT/ultragraphdb-gaps.md`) asked for is in the specification and carried Gen5
without a change of meaning: the statement root over entities and relations, union-typed roles, bitemporal types with
`@retracts`, `@on_retract` and the live-slice rule for every constraint, `@limit` with filters, per-type grouping and
lower bounds, `@acyclic` with `open` and `max_depth`, catalog types as participants, intervals with per-bound precision
and an interval index, `Geo` with `@uncertainty`, quantities and units with `Measure`, filtered derivations, modules,
constant defaults for `rank` and `inferred`, value entities for the tag index, and provenance links as `@set
@append_only` relations, which the encoding rule makes plain edges. The compiled form also gives Gen5 what the draft
expected of the engine: type-prefix ranges for every "every Router", key claims per type tree, and a coordination
report naming the memberships whose two constraints demand two anchors.

The conversion also found six defects in the reviewed draft that the static rules catch and a reader did not: an
attribute `rank: UInt32` redeclaring the inherited `rank: Rank` (`AuthorityOver`), an attribute `grounds` clashing with
the navigation every statement gains from `GroundedIn` (`Invalidation`), a function `Statement.units()` clashing with
the navigation `units` on `Organization`, a navigation `asserted` clashing with the engine's `asserted` interval, the
attribute `kind` declared with four different types, and `altitude` declared once with a fixed unit and once without.
That is the argument for the rules: a schema of this size is past what review holds in its head.

## B. Rough edges, by area

Every rough edge the exercise found on 2026-11-05 is closed. The changes to the language, B1 lexical structure and
names, B2 units and quantities, B3 annotations and the expression language, B4 types, attributes and identity, B5
relations, placement and encoding, and B6 functions and derivations, are in `SCHEMA.md` as of the same day, each
recorded in its section 17. The three findings that were decisions rather than specification changes are recorded
where they belong: personal data and privacy protection deferred until UltraGraphDB and DeepBrain have stabilised
(`SCHEMA.md` section 17; the Gen5 note's Later section; the gaps note's G12), the maintenance queue `Asserted.stale`
kept virtual as a standing limit of one owner per fact until the tile stage (the Gen5 note's derivations and
invariants; the gaps note's G10), and the conformance corpus of section E built once the language is final for
DeepBrain's current target (`SCHEMA.md` section 17). A second pass the same day, sections F to H, found two more
defects and resolved them.

## C. Gaps and design flaws

What the exercise found, in order of consequence; items 1 to 7 are resolved in `SCHEMA.md` on 2026-11-05 and item
8 is deferred by decision, as section B states.

1. **The key claim's placement** (B5). Every statement in Gen5 is keyed by `id`, so every insert pays a cross-partition
   claim unless the claim is placed by the entity's own hash. A rule change in the specification, and the stage 5.5
   `Bucket::of_value` stays for `@unique`.
2. **`@colocate(root)` against valid time** (B5, E23). The organisation tree, the one hierarchy DeepBrain needs local
   acyclicity for, cannot be declared. A rule change.
3. **The expression language's scope** (B3). Six of Gen5's checks fall back to the writer because the expression grammar
   lacks accessors, a quantifier and a dynamic type test. Every rule left in the writer is a rule the database does not
   enforce in every language, which is what UGS exists to prevent.
4. **`@ends(role)`** (B5). A decided Gen5 semantics with no declaration.
5. **Reserved words** (B1). Eight renames in one schema; cheap to fix and costly to leave, since every rename changes a
   vocabulary IRI.
6. **Name spaces for quantities and units** (B1).
7. **Symmetric node-encoded relations** (B5).
8. **Personal data** (B4), postponed knowingly.

## D. UGS against the core directive

UltraGraphDB aims for Ultra Correct, Ultra Performance and Ultra Scale: correctness, performance, scale, developer
experience, in that order, the order deciding the rare conflict. The specification is judged below in that order, with
Gen5 as the load.

### Correctness

What holds. Every invariant is declared, and the static rules are exhaustive enough to find six defects in a reviewed
draft. Validation is of the whole instance at commit, against one catalog version, with refusals that name the rule and
the instance. Bitemporal counting of the live slice, keys per type tree, types fixed for life, one canonical catalog
encoding, and a classification derived from declarations alone are all what the directive asks: the database, not the
developer, decides. The staging table keeps the engine honest about what it enforces, and `UGS0902` says so in every
report.

What does not yet. Three findings are correctness debts in the specification itself: the key-claim placement, which
is a performance defect that reads as a correctness one because the worked example states a locality that does not
hold; the tree rule, which refuses a correct schema; and the grammar bugs, which show the specification has been read
but never parsed. Two findings move rules back to the writer, which the directive's first rank forbids as a design
goal: the expression language's scope and the missing `@ends`. And one standing risk: a declared constraint the
deployment does not enforce (`UGS0902`) is reported, but a reader of the schema alone, which is what a `.ugs` file
invites, sees a promise the stage does not keep. A compiler flag refusing unenforced constraints, on by default in
production, would keep the schema and the engine in one truth.

### Performance

What holds. No stored `rdf:type`, and type tests as prefix-range checks. Links are edges, one quad each. Derived
buckets put a keyed entity where its key is checked (once the claim rule is fixed). Indexes and materialized closures
are written in the same local commit, with no second owner to keep in step. Count indexes exist only where a bound
needs them. Request typing decides a write's path before any read.

What it costs, and what Gen5 pays. Every relation kind of Gen5 lies below `Statement`, so every relation instance is a
node: one ID, two role quads, the envelope's attributes, and a registration at each participant's home the first time
its bucket group references it, where a link costs one quad. That is the price of "a relation is a statement", and the
directive's order says to pay it: identity for every relation is what makes it rankable, retractable and a participant.
The specification should state the ratio plainly in the encoding section, so a modeller chooses a link where the
epistemology is not needed, as Gen5's eighteen links do. Gen5 also indexes liberally, `@index(range)` on some seventy
attributes and `interval` on seven, each interval four entries; the coordination report should count index entries per
write operation so that amplification is visible when the schema is written, as coordination is.

### Scale

What holds. 65,533 types per tenant, 64-bit sequences per type, 2²⁴ buckets with 65,536 per tenant run, and
prefix sets that absorb modules without renumbering: nothing in the identity scheme bounds a DeepBrain deployment. Hub
participants, `Activity.generated*` over millions of readings and `Statement.incoming*` on every node, are bounded by
P24: inbound entries live with their subjects, and a referrer set holds at most 2¹⁰ groups per run, so a reverse read
fans out to a bounded set whatever the in-degree. `-> _` removes even that for roles never walked backward.

What it costs. Until stage 8 every segmented and coordinated path is refused, and a single tenant is spread
over every partition by design, so nearly every relation insert in Gen5 registers remotely and every `restrict`
retraction, cross-tree move and `@ends` across partitions waits for the sequencer. The classification makes that cost
visible per operation, which is the directive's intent; what it does not yet do is weigh operations by expected traffic,
which the operator's view in the stage review asked for. One structural limit stands: a derivation that reads other
partitions' state cannot be materialized before tiles, so DeepBrain's stale queue is a query, not a flag.

### Developer experience

What holds. Diagnostics in the compiler's style with a fix, a report that prices every constraint when it is written,
introspection with documentation, defaults, a worked example, and a staging table.

What does not. The reserved-word and built-in-name collisions are pure friction, eight renames of vocabulary that
clients will see as IRIs; the shared-attribute rule demands naming discipline the draft did not have; `multiplier: 1`
on every unit is noise; a function cannot order its result; and a schema author must hold two relation encodings and
their consequences in mind. None of these conflicts with the three ranks above it, so all of them should be fixed; the
directive puts developer experience last only where it would cost correctness, and here it costs nothing.

### Verdict

UGS as specified expresses Gen5 whole: 27 markers, none structural, and six of them defects in the draft that the
rules caught. Three rule changes are due before stage 6 builds against it, the key claim's placement, the tree rule under
valid time, and the expression language's accessors, quantifier and type test; `@ends` and the lexical fixes follow.
After those, the specification meets the directive's order: it puts every invariant in the database, pays performance
where identity requires it and nowhere else, bounds every structure that grows with the data, and leaves developer
experience the work of removing friction that costs the first three nothing.

## E. Proposal: a conformance corpus for the specification

The specification has been read and used, never parsed or executed; two grammar bugs and two rule gaps survived both.
Ultra Style's test applies to a specification as to code: a rule no tool can check is a wish. The corpus below makes
every rule of `SCHEMA.md` checkable before and apart from any implementation, so that stage 6 is built against it and a
second implementation, a client-side checker or a language server, can be held to the same text.

### E1. Principles

1. **Independent of the implementation.** The corpus is data: UGS sources, expectations and goldens in files under
   `specification/conformance/ugs/`, exported by a Bazel `filegroup` any crate may depend on, as the TOML subset's
   `toml-test` and the protocol's golden frames are. An expectation is written from the specification's text, or from
   an independent reference (the xxHash reference for buckets), never copied from an implementation's output; a golden
   an implementation produced is frozen only after review and says so in its header.
2. **One case per rule, both ways.** Every diagnostic code has at least one source that must be refused with exactly that
   code at a stated location, and a twin that applies the diagnostic's own fix and must compile. The twin tests the fix
   text as much as the rule.
3. **Traceable.** A manifest maps every rule to its cases. A test reads `SCHEMA.md`, collects every code in its tables,
   and fails when a code has no reject case or no accept twin, so a rule added to the specification without a case
   fails `make test`.
4. **The specification's examples are cases.** Every fenced UGS block in `SCHEMA.md`, and in the design notes that
   claim to be UGS, is extracted and compiled; a block meant to fail carries a marker naming its code. The worked example
   of section 16 and the Gen5 schema are the two large accept cases, with goldens.
5. **Executable by Bazel, hermetically.** Each layer is a `rust_test` target in the stage 6 crate over the filegroup,
   tagged `unit-test` or `bdd-test` as the repository runs them; nothing reaches outside its declared data.

### E2. Layers

| Layer | Cases | Expectation | Source of truth |
|---|---|---|---|
| Lexical and syntax | `syntax/accept/*.ugs`, `syntax/reject/*.ugs` | Accept: parses and pretty-prints to a source that parses to the same tree. Reject: `UGS00xx` at `line:column`, in a sidecar `*.expect.json` | Sections 3 and 4; one case per token kind, per reserved word in each declaration position, per contextual keyword as a name, per bound of section 3.7 |
| Static semantics | `static/<code>/reject.ugs`, `static/<code>/accept.ugs` | The code and location; the twin compiles | Section 10.4's table: every code from `UGS0101` to `UGS0905`, warnings and notes included |
| Compilation | `compile/<name>/schema.ugs`, `expected.json` | The compiled catalog as a logical JSON document: types with prefixes and prefix sets, encodings and anchors, predicates, roles, navigations, enums, units, derivations, functions, graphs, the classification table and the report | Sections 7, 9, 10 and 11.1; goldens hand-written for the small cases, frozen after review for section 16 and Gen5 |
| Canonical encoding | the same schemas | Once the catalog's byte encoding is specified: the bytes, and `decode(encode(c)) = c` with any other byte sequence refused | Section 10.2; the schema id of `PROTOCOL.md` section 11.7 over the Describe body |
| Identity | `identity/*.json` | For (tenant, type, key): the canonical key bytes, the bucket, the IRI; for an IRI: the type, the key read back, the bucket; percent-encoding edge cases, composite keys, `@iri` templates, handles | Section 7; buckets from the xxHash reference, as `system-identity`'s golden values |
| Standard views | `views/<name>/vocabulary.nt`, `shapes.nt`, `graph_type.gql` | The RDF vocabulary and SHACL shapes as sorted N-Triples after canonicalisation; the GQL graph type as text | Section 11; hand-written for the small cases |
| Commit-time rules | `commit/<code>.feature` | Gherkin: Given a schema, When a write in N-Quads or a UQL statement, Then refused with `UGS1xxx` naming the instance and rule, or accepted and read back; the report classifies the write as the table says | Sections 8 and 9; one scenario per refusal `UGS1001` to `UGS1035`, one per row of section 9.3, run by the repository's cucumber harness with step definitions in the implementing crate |
| Evolution | `evolution/<name>/v1.ugs`, `v2.ugs`, `expected.json` | The change's kind, the prefixes before and after (stable), the report's path changes, or `UGS2xxx` | Section 13 |
| Properties | generators, not files | A grammar-derived generator of sources: every generated source compiles or yields a diagnostic inside the source, never a panic; parse, print, parse is the identity; prefix allocation is a function of the install sequence alone; encode is injective | Sections 3, 4, 7.1, 10.2; the structure-aware fuzzer ROADMAP plans for the query parsers, run by `scripts/fuzz.sh` with the corpus as seeds |

### E3. Layout

```
specification/conformance/ugs/
  BUILD.bazel                 filegroup "corpus", every file below
  manifest.json               rule or code → case paths; sections → cases for rules without a code
  syntax/   static/   compile/   identity/   views/   commit/   evolution/
  seeds/    the aviation schema, DeepBrainSchema.ugs, ServiceRadarExtension.ugs, the gaps exercise's E cases
```

An expectation file is JSON, which the test harness already reads through `load_golden`. A `.ugs` source carries its
expectation in a leading comment as well, `// expect: UGS0702 at 12:3`, so a case is readable on its own and the sidecar
is checked against the comment.

### E4. Gate

* `make test` runs every layer; a stage 6 task is done when its layer passes, in the order syntax, static, compilation,
  identity, views, commit, evolution.
* A change to `SCHEMA.md` that adds or alters a code, a grammar production or a section 9.3 row ships with its cases in
  the same commit; the manifest check and the extracted-examples check are the tools that hold that rule.
* The property layer runs in CI at the fuzzer's time box and its findings are minimized and committed under the
  implementing crate's `testdata/fuzz_regressions/`, as every decoder's are.
* Mutation testing of the implementing crate waits for stage 9.6, as decided; the corpus is what makes a survivor
  there a gap in the specification rather than in a test's imagination.

### E5. Size and order

About 140 static codes give some 300 syntax and static cases; the 35 refusals and 20 classification rows give some 60
scenarios; the compile and view goldens number one per section of the specification plus the two seeds. The first
cases to write are the ones this exercise already produced: the 27 markers of `DeepBrainSchema.ugs` are 27 reject
cases with their twins in the file as it stands, and the six draft defects are six static cases. The corpus is then
built outward from the grammar, one section at a time, before the parser is.

## F. Expressing Gen5 natively

Gen5 was drafted against the design note, before the specification existed, and carries Gen4's shape: every statement
keyed by a client-minted `id`, one envelope root with two generic roles, every relation kind beneath it. A second pass
on 2026-11-05, asking what a schema designed for UGS would do, finds that four of its costs are the draft's choices and
not the language's, and two more are defects in the specification that the first pass missed. First the defects, since
they decide what Gen5 may do at all:

* **A subrelation may not choose its anchor.** `SCHEMA.md` inherits `@colocate` and refuses one on a subtype
  (`UGS0701`), so `SubOrganizationOf : Relation @colocate(root)` is refused, and every kind below `Relation` is anchored
  at the subject because the root is. The first pass marked E23 on the cardinality rule and missed this. Section G1
  resolves it.
* **A keyed relation instance has two homes.** Its IRI is minted from its key (section 7.3), its bucket is its
  anchor's (section 9.1), and section 7.3 promises that any node finds an instance's bucket from its IRI. For a keyed
  relation that promise is false. Section G2 resolves it; the reformulation below avoids it.

The reformulations, each native to the language as it stands:

| Gen5 as drafted | Native form | What it resolves | What DeepBrain absorbs |
|---|---|---|---|
| `Statement @key(id)`: every entity and every relation instance carries a client-minted `NodeRef` key | `@key(id)` on `Node`; `Statement` and `Relation` keyless, a relation instance named by its handle IRI; `Reading` keyless too | The two-homes defect never arises; a relation insert pays no key claim, and a reading none, where every one of Gen5's 69 kinds and every observation-rate value paid one; a relation's bucket is its anchor's by one rule | `NodeRef` names entities; a relation is named by the handle its insert returns (`returning { iri }`), which is stable for life. `Placed` and the catalogue questions that take a relation's `id` take its IRI |
| `Relation @multi` with `@limit(count() <= 1 per (subject, type) where rank = preferred)` on the root, so every kind is subject-anchored and `UniquePreferred` is local everywhere | Keep the root bound; declare `@colocate` on the kinds whose own constraint reads the object side once G1 admits it: `MemberOf @colocate(collection)` for `LiveUnit`, `SubOrganizationOf @colocate(root)`, `Holds @colocate(post)` for one holder at a time | `LiveUnit` and the one-holder rule become local, where the draft left them coordinated; the report names, per kind, that `UniquePreferred` for that kind reads across partitions instead, a choice the modeller makes kind by kind | Nothing |
| `derive Asserted.stale = grounded_in(with retracted, not live or valid.end is some or rank = deprecated) exists`, which must read each ground's state and so stays virtual | `derive Statement.closed = not live or valid.end is some or rank = deprecated @materialized @index(range)` on the ground's side, local to its partition; the queue is `Asserted(inferred, some grounded_in(with retracted, closed))`, a reverse read through the referrer set over an indexed Boolean | The standing limit is sidestepped: the maintained flag sits with the state it reads, and the queue is a join of two indexed reads with no cross-partition maintenance | The maintenance queue is read as a query; `StaleMarking` as an invariant is the pair of the flag and the read |
| Secondary identities as `@unique`: `contextoid_id`, `context_id`, `teloid_id`, `causaloid_id`, `action_identifier`, each a claim at the value's hash partition | Where the number is the entity's real identity, make it the key: `Context @key(context_id)`, `Teloid @key(teloid_id)`, `Action @key(action_identifier)`; keep `@unique` for the alternate names | A key claim is local; a `@unique` claim is segmented at another partition in general | The engine's numbers name those entities, which `Slice` and the hydration already do |
| `GeneratedBy.activity -> generated*`, `InScenario.scenario -> statements*`: hub navigations an activity with millions of readings and a scenario with every statement gain | Keep: P24 bounds a hub's referrer set to 2¹⁰ groups per run, and both are walked backward (`results`, a scenario's statements). `-> _` only where a role is never walked backward, which Gen5 has none of | Nothing to resolve; the cost is bounded by construction | Nothing |
| 18 provenance links as `@set @append_only` relations | Keep: they are edge-encoded, one quad each, which is what a link should cost | Nothing | Nothing |

What stays as a cost of DeepBrain's own design and not of the draft: every relation is a statement that other
relations name, so every kind below `Relation` is node-encoded, one node, two role quads and the envelope's attributes
per instance, against one quad for a link. Section G3 lowers that cost without changing the model.

## G. Evolving UGS for what remains

Each proposal names the directive rank that decides it. Correctness first: a rule that cannot be declared is a rule the
writer holds, which the language exists to prevent; performance second, where identity and locality allow a cheaper
encoding; scale third; developer experience last, and only where it costs the first three nothing.

### G1. A subrelation chooses its anchor

**Rule.** A subrelation may declare `@colocate(role)` or `@colocate(root)` over an inherited or own role. Its
instances take that anchor; a constraint inherited from the supertype whose state lives at another role is, for that
kind, classified as section 9.3 classifies a constraint on a non-anchor role, and the report lists it under `UGS0904`
as a placement the subtype chose. The supertype's own navigation names keep working: a supertype navigation reads
instances of every subtype wherever each is anchored, by the same reverse read it makes today. `UGS0701` shrinks to an
unknown role.

**Why.** Correctness: `Hierarchy` and `LiveUnit` are DeepBrain's two invariants that need a local check, and both read
the object side of a kind below a subject-anchored root; without the rule they cannot be declared local, and the
draft's `@colocate(root)` is refused outright. Performance: the choice is per kind, so the common kinds keep the root's
locality and the constrained ones buy their own. The classification machinery already handles a non-anchor constraint;
the rule only lets a subtype move the anchor.

### G2. A keyed relation's home

**Rule.** A relation type with `@key` is anchored at its key: its instances take the bucket `Bucket::derive` gives the
declaring prefix and the canonical key, as a keyed entity does, and the relation declares no `@colocate` (`UGS0702`
extended). Section 7.3's promise then holds for every keyed instance. Every constraint on such a relation that reads a
participant is classified as reading another partition's state. A relation type without a key is anchored as today.

**Why.** Correctness: one rule for where an instance lives, derivable from its IRI, is what the whole placement model
rests on; two rules for one instance is the defect. The price is stated, not hidden: a keyed relation gives up
colocation, and the report says so. Gen5 avoids the price by keying entities only (section F).

### G3. Edge encoding through the reifier

**Rule.** A binary, single-valued relation is edge-encoded even when its instances may fill roles: the instance is the
asserted triple with its reifier, the reifier a statement ID carrying the relation's prefix, and a role naming the
instance holds the reifier as its object term. The reifier is allocated at insert for such a relation, as it is for a
`@multi` or attributed one. Node encoding remains for relations with three or more roles or a set-valued role. The GQL
view keeps presenting a relation that fills roles as a relation node, since a GQL edge cannot be an endpoint; the view
is derived from the encoding and no longer decides it.

**What it costs and saves.** Per instance, one triple and one statement record in place of one node and two role
quads: two quads fewer, and the same two registrations. For Gen5, 61 of the 69 kinds below `Relation` are binary and
single-valued. The quad layout already stores statement IDs with their properties and takes a system ID as an object
term, so the storage change is in the encoding rule and the instance assembly of section 8.1, which must accept a
reifier where a role expects a participant.

**Why.** Performance, within correctness: the instance keeps its identity, its prefix and its envelope, and every
constraint reads the same state at the same anchor. Scale: fewer quads per relation is fewer bytes per epoch on the
path the directive's note measures. It departs from the mapping contract's rule that a statement ID exists only for
properties or parallel edges by one case, a statement that fills a role, which RDF 1.2 reification is for.

### G4. The report prices index writes

**Rule.** The coordination report counts, per write operation, the index entries the operation writes: value, interval
(four per attribute), count, fulltext and vector, so write amplification is visible when a schema is written, as
coordination is. A note `UGS0906` lists the ten most expensive operations.

**Why.** Performance: Gen5 indexes some seventy attributes by range and seven by interval, and nothing in the language
today tells the modeller what each costs per commit. The report already exists; this adds a column.

### G5. What stays as it is

* Every relation registered at its non-anchor participants, and every non-anchor navigation a reverse read: P24's
  design, bounded by the referrer set, and the price of one owner per fact.
* A `@unique` claim at the value's hash partition: segmented by nature, since uniqueness across a tree cannot be
  checked where the writer is; section F moves the real identities to keys instead.
* Node encoding for n-ary relations and set-valued roles.
* A derivation that reads other partitions' state: virtual, with section F's inversion as the idiom.

Taken together: Gen5 reformulated as section F describes, on the language as section G1 to G3 would make it, declares
every invariant it held in the writer except the three that depend on the writer's own state, pays a key claim only
for entities, pays two quads fewer for most relations, and keeps every constraint local that its modeller chose to
anchor. That is the directive's order applied: correctness in the declarations, performance in the encoding, scale by
bounded fan-out, and the developer's choice of anchor per kind made visible by the report.

## H. Resolution and application

Sections F and G overlap where a Gen5 choice and a language change bear on one fact. Resolved as follows, and applied
to `SCHEMA.md` and to the Gen5 files on 2026-11-05.

| Conflict | Resolution |
|---|---|
| G2 (a keyed relation lives at its key) against F1 (Gen5 keys entities only) | No conflict in content: G2 is the language's rule, F1 is Gen5's choice to never meet its price. Both stand |
| G3 (edge encoding through the reifier) against G2 and `@colocate(root)` | G2 decides first: a keyed or root-colocated instance takes a bucket that is no participant's, so an edge whose subject lives elsewhere would have two owners. Edge encoding applies to unkeyed binary kinds without `@colocate(root)`; the rest are nodes |
| G1 (a subrelation chooses its anchor) against the root's `UniquePreferred` | A kind that anchors at its object gives up the root bound's locality for that kind, and the report says so. The choice is the modeller's, kind by kind: Gen5 takes it for `MemberOf`, `Holds` and `SubOrganizationOf` |
| F4 (the engine's numbers as keys) against F1 | Withdrawn: a type tree has one key (`UGS0302`), and every `Node` is keyed by `id`. `contextoid_id`, `context_id`, `teloid_id` and `action_identifier` stay `@unique`, a secondary claim each; a deployment that wants them local names them in `id` instead, as the `NodeRef` convention `CTX:42` allows |
| F3 (the closed flag on the ground's side) against G5 (cross-partition derivations stay virtual) | Consistent: the flag is local and materialized, the queue is a virtual read through it. It needs one rule the grammar lacked, `is T` narrowing the element for the filter items after it, which the UQL design already assumes; added to section 4 |

Priority of application, by the directive's order: G2 (one rule for where an instance lives), G1 (two invariants
declarable local), G3 (two quads fewer per relation), G4 (index writes priced), G5 (no change). All four are in
`SCHEMA.md`'s sections 6.4, 6.7, 7.1, 8.1, 9.1, 9.4, 9.5, 10.4, 11.1, 11.3, 12.3, 15, 16 and 17.

The Gen5 files are reworked on the updated language: `DeepBrainSchema.ugs` keys `Node` alone, names relation instances
by their handles, chooses anchors for the three constrained kinds, materializes `closed` on `Asserted` and reads the
stale queue through it, restores every name the first pass had to rename for a reserved word or a built-in type, and
drops every marker, since nothing marked remains. `schema_graph_gen5.md` is rewritten as the standalone Gen5 note over
that file, with no Gen3 or Gen4 reference, which the user shelved the same day.
