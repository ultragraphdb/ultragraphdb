# Specification gaps found while drafting stage 6, with recommendations

* Status: accepted by the user on 2026-11-10, all thirteen, and applied the same day to `SCHEMA.md`, `PROTOCOL.md`,
  `QUERY-ULTRA-QL.md` and `SCHEMA-APG.md` at the sections each entry names (tasks 0.2 and 0.7). The change's capability
  specifications are written to them.
* Read for them: `specification/schema/SCHEMA.md`, `SCHEMA-APG.md`, `SCHEMA-EVOLUTION.md`,
  `specification/binary_protocol/PROTOCOL.md` in full, and `specification/query/QUERY-ULTRA-QL.md` sections 1, 3 to 6,
  12, 13 and 17 to 19.

Each recommendation is derived from four principles the specifications already hold:

| | Principle | Where the specifications state it |
|---|---|---|
| P1 | One fact has one owner and one stored form | P24; `SCHEMA.md` section 9.1; `SCHEMA-APG.md` proposition 17.3 |
| P2 | One value has one encoding, and one rule one refusal number | `PROTOCOL.md` section 1; `SCHEMA.md` sections 1 and 10.2 |
| P3 | A governed tenant's catalog is the compilation of its sources and nothing else | `SCHEMA.md` sections 10.1 to 10.3 |
| P4 | A stage refuses what its machinery cannot decide, and serves what it already can | `SCHEMA.md` section 14; `PROTOCOL.md` section 11.2 |

## Resolved in `SCHEMA.md` on 2026-11-10

**The number refusing a revision.** Section 8.4 rule 2 refused a revision of an `@append_only` instance with
`UGS1013` and held a key, an `@immutable` attribute and a required role "as section 8.2 holds them"; section 8.7 gave
all four `UGS1025` for `@bitemporal` types. By P2 a refusal number names the rule broken, the same for every
mutability class: a client then acts on a number without knowing the type's class, and each constraint of
`SCHEMA-APG.md` section 14 raises one refusal. A revision is how an update is stored, not a rule.

- A revision's refusals are the rows' own: `UGS1007` (key), `UGS1012` (`@immutable`), `UGS1013` (`@append_only`), as
  sections 8.2, 8.3 and 8.4, `SCHEMA-APG.md` section 14.3 and `QUERY-GQL-CORE.md`'s writes already had them.
- One condition `UGS1025` named had no other number, for any class: replacing the participant of a required,
  single-valued role. `UGS1025` now names that rule alone, for every relation, as RoleFixed; section 8.2 gained its
  row.
- `UGS1013`'s text now covers the retraction section 8.3 refuses with it.
- Section 17 records the decision. `SCHEMA-APG.md` section 14.3 gained the transition constraint for a required role's
  participant. The Lean model needs no new definition: `Catalog.State.FieldsFixed` takes any fields of a relation's
  record, a required role among them, and its comment now names `UGS1025`.

## The seven gaps of the first draft

### 1. A keyed instance created under a subject IRI its key does not mint

- *Silent:* `SCHEMA.md` section 7.3 mints the IRI from the key; section 8.7 has no refusal for a creation under
  another IRI. `PROTOCOL.md` section 4.4 refuses a forged handle, and says nothing of a keyed IRI.
- *Principle:* P1 and P2. An instance has one IRI, and every node finds its bucket from that IRI and the catalog
  (section 7.3). A creation under another IRI would route by a bucket the key does not hash to.
- *Recommendation:* a new refusal, `UGS1038` IriNotMinted: a write creates an instance under a subject IRI its type
  does not mint for it. It covers a keyed type's instance whose subject is not `{base}{KeySpace}/{key}` or its
  template for the key attributes written, and a keyless type's instance whose subject is an IRI rather than a blank
  node. A key attribute written with another value on an *existing* instance stays `UGS1007`.
- *Rejected:* reading the key from the IRI and filling the attributes in. It gives one write two spellings, and a
  typing error in an IRI would create an instance rather than be refused.
- *Amends:* `SCHEMA.md` sections 7.3, 8.1, 8.2 and 8.7; `PROTOCOL.md` section 11.12's binding table, whose "an IRI's
  bucket computed from its text" holds for a `Resource` alone: a keyed IRI's bucket is computed from the key space
  and the key it states (gap E below).

### 2. A write-path declaration in a governed tenant

- *Silent:* `PROTOCOL.md` section 11.13 commits change records 8 to 10 as a catalog version; `SCHEMA.md` never says
  whether a governed tenant takes them.
- *Principle:* P3. Describe returns the sources byte for byte and the schema id covers the compiled entries; a
  declaration outside the sources would make the catalog something no source compiles to.
- *Recommendation:* in a governed tenant a change record of kind 8, 9 or 10 is refused with code 7, `detail` the
  record's index, the message naming Install. A Drop of kind 1 naming a predicate a schema declares is refused with
  code 7 naming stage 9.5, since removing a declaration is section 13.2's destructive change; a Drop of kind 1 of a
  predicate `@open` admitted, and of kinds 2, 3 and 4, stay as they are. A Drop of kind 4 is how stage 6 removes a
  tenant's schemas: it sets the catalog to the initial one.
- *Amends:* `PROTOCOL.md` sections 6.5, 6.6 and 11.13; `SCHEMA.md` section 6.1.

### 3. A quad write of a triple the view derives

- *Silent:* `SCHEMA.md` section 11.2 answers an inverse navigation, a node-encoded relation's navigation property and
  a super role's triple from stored triples; section 8.7's `UGS1029` names only `derive` names.
- *Principle:* P1. `SCHEMA-APG.md` definition 17.1 leaves these triples out of `R(G)`, so `R⁻¹(R(G)) = G` holds only
  if a write names the stored form. Normalising `organization av:assets asset` to `asset av:owner organization`
  would also bind the transaction to the wrong subject's partition.
- *Recommendation:* `UGS1029` DerivedWrite covers an insert or delete of any triple the view derives, its message
  naming the stored triple to write.
- *Amends:* `SCHEMA.md` sections 8.2, 8.7 and 11.4.

### 4. A UQL request in a tenant with no schema

- *Silent:* `QUERY-ULTRA-QL.md` assumes a schema.
- *Principle:* UQL takes every name from the schema, so a tenant without one has no working graph.
- *Recommendation:* `UQL0104`, the message saying the tenant holds no schema.
- *Amends:* `QUERY-ULTRA-QL.md` section 18's `UQL0104` text.

### 5. A relation selector that binds no anchor role

- *First draft:* refused with `UQL0901`.
- *Principle:* P4. `PROTOCOL.md` section 11.11 already serves a reverse read, an unbound subject with a bound IRI
  object, through the object's referrer set, for a read-only transaction across partitions and for a read-write one
  whose partition owns the tenant's run.
- *Recommendation:* the subset accepts a relation selector binding any of its roles by point selectors, or none. A
  bound anchor role is a forward read; another bound role a reverse read under section 11.11's rules, code 23
  included; no bound role a scan of the type.
- *Amends:* `QUERY-ULTRA-QL.md`'s stage 6 marks; the `uql-crud` specification is written so.

### 6. A Prepare of a UQL text

- *Silent:* `PROTOCOL.md` section 8.1 lists `language` 0 to 2; section 9.4's language table already holds 3.
- *Principle:* P2 and P4. One table of languages serves Query, Prepare, code 29 and warnings; a prepared text needs
  the planner and the staleness rules of the query core.
- *Recommendation:* sections 7.4 and 8.1 take `language` 3. Section 11.2 serves a Query of `language` 3 from stage 6
  within the subset and refuses a Prepare of `language` 3 with code 7 naming the stage that builds UQL.
- *Amends:* `PROTOCOL.md` sections 7.4, 8.1, 9.1 (a UQL mutation's reply, a Result of no columns and no rows, as a GQL
  statement's without `RETURN`) and 11.2.

### 7. Entity types a tenant with no schema declares, and the preorder number

- *Silent:* `PROTOCOL.md` sections 4.6 and 8.3 still carry an entity type's preorder number in `extra`; `SCHEMA.md`
  section 15 replaces preorder numbers by prefixes. Since stage 5.5 every such tenant's entity is a `Resource`, so
  the number names nothing in the store.
- *Principle:* P2. A field on the wire names something, and a type's place in a walk changes when a sibling is
  declared, which an identity never does.
- *Recommendation:* `extra` is 0 in every change record. A type declared through the write path holds no prefix; a
  type a schema declares holds its prefix in its Type entry, Describe's kind 3.
- *Amends:* `PROTOCOL.md` sections 4.6 (the field and step 5) and 8.3 (section 5's text). The golden
  `describe_reply.frame` holds no kind-9 record, so the schema id's golden value stands.

## Further gaps found reading `PROTOCOL.md` in full

### A. A `@bitemporal` edge-encoded instance may have no statement record

`SCHEMA.md` section 12.4 stores `asserted` as two quads on the instance; section 11.1 allocates an edge-encoded
instance's reifier only when it carries attributes, is `@multi`, has a tree of several concrete types, or is named by
a role. A `@set` `@bitemporal` binary relation inserted without attributes then has nowhere to hold `asserted` and no
IRI to retract by.

- *Recommendation:* section 11.1 allocates a reifier also when the relation's type is `@bitemporal`. Every Gen5
  relation below `Relation` already has one through `@multi`.

### B. A participant's retraction is invisible to a reference

`PROTOCOL.md` section 13.15's get-only `status` is absent, live or deleted. `UGS1019` refuses a retracted
participant, which a home in another partition cannot report.

- *Recommendation:* `status` 4, retracted: a `@bitemporal` instance whose `asserted` has ended, `entity` holding its
  ID. The registrant refuses with `UGS1019`.

### C. One-frame writes and operations 1 to 4 in a governed tenant

`PROTOCOL.md` section 6.6 refuses a DirectWrite of a unique predicate and names ConditionalWrite, whose claim is an
operation that creates an untyped node. A keyed instance of a declared type fits neither.

- *Principle:* P4, and section 6.6's own: a request named direct runs no condition the client did not ask for.
- *Recommendation for stage 6:* a DirectWrite in a governed tenant carries changes whose predicates the catalog's
  request-typing facts mark blind (`SCHEMA.md` section 9.6), and is refused with code 7 otherwise, naming a read-write
  transaction. A ConditionalWrite carries such changes with operations 3 to 6. Operations 1 and 2 are refused in a
  governed tenant with code 7: they create a node of no declared type. A keyed creation and a `@unique` write take a
  read-write transaction. Backlog item 15 decides the one-statement form with the query core.

### D. A first schema installed into a tenant that holds data

Data written before a tenant's first version is `Resource` data under default entries. `SCHEMA.md` does not say what
becomes of it once the tenant is governed.

- *Recommendation:* an Install is refused with code 7 when the tenant's catalog holds a write-path declaration or a
  default-entry record, the message naming Drop of kind 4.
- *Consequence, applied with it:* a Drop of kind 4 removes the tenant's default-entry records with its catalog
  (`PROTOCOL.md` sections 6.5 and 11.13), where a drop left them before; otherwise a dropped tenant could take no
  schema.
- *Decided by the user, 2026-11-10:* a write in the window between the Install's check and its version's effective
  epoch is refused. Applied as a fence: the frontend refuses every write to the tenant with code 31 (Fenced), retryable
  after a delay, from before the check until the version takes effect (`PROTOCOL.md` sections 8.4 and 9.4; `SCHEMA.md`
  section 10.1).
- *Strengthened in applying it:* a default-entry record is asked for after its commit and without waiting, so the
  catalog alone cannot show that a tenant is empty. The check therefore also reads the tenant's partitions for a
  statement, after the fence is up and the commits in flight are answered.

### E. Routing a keyed IRI

`PROTOCOL.md` section 11.12 binds a transaction by "an IRI's bucket computed from its text and the tenant".

- *Recommendation:* the table's first row reads the subject's bucket as `SCHEMA.md` section 7.3 gives it: a keyed
  IRI's from the key space and key it states, a handle's from its digits, any other IRI's from its text.

### F. `iri` of an edge-encoded instance without a reifier

`SCHEMA-APG.md` section 15.1 leaves such an instance unnamed; `QUERY-ULTRA-QL.md` section 6.3 gives every instance
an `iri`.

- *Recommendation:* reifier allocation stays per instance, as `PROTOCOL.md` section 4.6 gives a statement its ID with
  its first property. `iri` is `Option<Iri>` on an edge-encoded type whose every instance is not guaranteed a reifier
  by its type, and `None` for an instance without one. Such an instance is deleted by the delete of its quad, change
  record 3, and is never retracted (gap A).

### G. The name of an edge-encoded instance's reifier (open)

`SCHEMA.md` section 7.3 names every relation instance without `@key` by a handle IRI, `{base}{Type}/{handle}`, and
section 15 has a handle "held in the dictionary both ways as a reifier's skolem IRI is"; `PROTOCOL.md` sections 9.1
and 10.2 name a statement's reifier by a skolem IRI under `.well-known/genid/`, tagged as a blank node. Found while
writing Operation kinds 5 and 6, which name an edge-encoded instance by its reifier's IRI.

- *Open:* is the reifier of a declared relation's edge-encoded instance named by a handle IRI of its type, tagged as
  an IRI in the reifier column, with the skolem form kept for statements of no declared relation?

## Also applied on 2026-11-10

- The preparation's G2: an Install's `@index(ann)` accepted and unenforced (`SCHEMA.md` sections 10.4, 14 and 17;
  `PROTOCOL.md` section 11.13).
- Evolution at stage 9.5 and one Install per tenant until then (`SCHEMA.md` sections 10.1, 13, 14, 15 and 17;
  `PROTOCOL.md` sections 8.4, 11.2 and 11.13).
- Operation kinds 5 and 6 (`PROTOCOL.md` sections 6.3, 7.3, 11.6 and 12.2; `SCHEMA.md` section 8.1;
  `QUERY-ULTRA-QL.md` section 4.1).

## What the user decided on 2026-11-10

| Question | Decision |
|---|---|
| Schema evolution | All of it at stage 9.5, in place and by migration: it needs the full cluster |
| A wire form for delete and retract | Added: Operation kinds 5 and 6 (design decision 4) |
| The `regex` dependency | Accepted, for now |
| The `sr` module | Left out of stage 6 |
| Where the schema crates live | A new group folder, `ultragraphdb_schema/` |
