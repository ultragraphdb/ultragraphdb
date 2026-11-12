# The corpus's readings

* Status: 2026-11-10, written into the specification files; for the user's review. Writing the conformance corpus and
  the instance corpus (tasks 0.9 and 0.10) found places where the specification was silent, ambiguous or
  self-contradictory. Each was settled as below, by the reading the corpus took where its authors agreed and by the
  repository's principles where they did not, and written into `SCHEMA.md`, `CATALOG.md`, `QUERY-SPARQL-CORE.md`,
  `PROTOCOL.md`, `SCHEMA-APG.md` and this change's specifications (`SCHEMA.md` section 17, "Corpus readings").
* Corrections applied after this list: UGS0904 is raised only when the anchor keeps another anchor-dependent
  constraint local (P-1); a module carries its own `prefix` header as a schema does (R-9).

Each item names where it lands. "The corpus's reading" means the reading the corpus already encodes; the specification
is written to it. Line numbers are SCHEMA.md's as of this pass and may drift by a few.

## Placement and the report (SCHEMA.md 9, CATALOG.md 5.9)
P-1 Anchor. 9.1 governs: a relation's anchor is the role `@colocate` names, else its first role. 9.4 step 4 is
    rewritten to take that anchor: a constraint another anchor would make local is routed through coordination and
    reported with UGS0904, as section 16's Supply is. No compiler choice of anchor remains.
P-2 9.3 gains a row: `cascade` on a role whose relations are all anchored on it reads the instances naming the
    participant, owned by the participant's home: coordination-free, the deletes being writes in that partition. The
    row "`restrict`, `restrict open`, `cascade` otherwise" stays for the rest.
P-3 9.3's row "Cardinality on another role's navigation" is "Coordinated"; it suggests nothing. UGS0903 is
    `restrict`'s alone (10.4, 9.5).
P-4 9.4 step 1: a `@bitemporal` type lists retract in place of delete; its updates are revisions and are listed.
P-5 9.5's subject examples name concrete types: `Engine.update.status`, `Engine.delete`.
P-6 CATALOG.md 5.9 `indexes`: a materialized derivation's index entries are not counted.
P-7 A `UGS0904` record: one per relation, its location the coordinated constraint where the relation declares it,
    else the relation's name; fix none. A `UGS0903` fix inserts ` @colocate(role)` after the relation's head. State both
    in 10.4's rows.
P-8 A symmetric relation's navigation name, declared on both roles, gives one UGS0905 record, at the second role.
P-9 `relation=` in UGS0905 is the qualified name; fix the ugs-semantics scenario in
    `openspec/changes/stage-6-6-ugs-data-definition/specs/ugs-semantics/spec.md` accordingly.

## Identity and bounds
I-1 Declared types per tenant: 65,532, prefixes 3 to 65,534. Prefix 65,535 is never allocated, so `prefix_next` and a
    prefix range's end fit 16 bits. 3.7's table, 7.1, CATALOG.md section 2's `prefix_next` rule (at most 65,535).
I-2 Decimal(p, s): add `s ≤ p` (5.1). The keyable-types list (6.3) names `Decimal(p, s)`, never a bare `Decimal`.
I-3 UGS0206's condition reads as 5.1 lists Interval's element types: an `Interval` over a type that is not `Date`,
    `DateTime`, `LocalDateTime`, an integer type or a `Decimal`.

## Lexical and grammar (SCHEMA.md 3, 4, 6.12)
L-1 A location in text that is not UTF-8 is the first invalid byte, one column, length 1 (3.1, 10.4).
L-2 An unterminated string, comment or IRI is located at its opening delimiter (UGS0002).
L-3 A number token is the longest run of digits, `_`, one `.` and an exponent; a run breaking 3.5's form is UGS0004.
L-4 A string's 4 KiB bound counts its text between the quotes as written (3.7).
L-5 An over-long line is located at the whole line (UGS0013).
L-6 `schema` is reserved; its one use as a qualifier is `schema.EntityType<T>` and the like. `import … as schema` and
    `prefix schema` are UGS0110, the specific code.
L-7 A literal in an annotation argument or a default may carry a leading `-` (3.5).
L-8 Module enum extension: the grammar's `enum` takes a `qident` in a module, `enum deep_brain.Rank { … }`, adding
    members to the schema's enum (6.12); a qualified name elsewhere is refused with the code 6.12 gives, else UGS0006.
L-9 A module declares relation types only as subtypes of its schema's (6.12's list reads so; UGS0106 holds).
L-10 A record field takes the value checks `@pattern`, `@range`, `@values` and `@check` (5.4 and 6.6 agree).
L-11 A bare identifier in an expression where an enum value is expected is a member of that enum (4).
L-12 A cycle (UGS0105, UGS0118) is one diagnostic, at the first of its declarations in source order.

## Codes
C-1 An undeclared participant type name is UGS0201, an unknown type name, as an undeclared parent is. UGS0402 is a
    declared name that is not an entity, relation or statement type, a repeated union member, or a member that is a
    subtype of another member ("nested", 6.5).
C-2 10.4's example (`aviation.ugs:44`, `:49`) cites the lines of section 16's block: check and correct to its lines.

## Commit (SCHEMA.md 6.3, 8)
K-1 8.2's order: a value's type before any check that reads it, so the datatype row precedes `@unique`; a key's
    presence (UGS1006) before required presence (UGS1005), the more specific first; UGS1036 and UGS1037 gain rows at
    their places by the same principle.
K-2 6.3: a second concrete type written on an existing keyless instance is UGS1004; on a keyed type's minted IRI it
    is a creation of a duplicate, UGS1008 (8.1, "Creation").
K-3 A blank node given attributes and no `rdf:type` is refused with UGS1002 (8.1).
K-4 A set-valued role with no participant, at creation or after a removal, is UGS1016; UGS1015 is a required
    single-valued role.
K-5 A write naming a tombstoned entity as its subject is refused with PROTOCOL.md code 30; naming it as a participant
    is UGS1019. Reconcile 8.5 with PROTOCOL.md code 30's text accordingly.
K-6 A reifier written by a client carries its relation's concrete type (8.1); QUERY-SPARQL-CORE.md's example near its
    line 294 gains the type, and its line 288 refuses a keyed instance under a blank node (UGS1038, 7.3).
K-7 The RDF view (11.2) holds a materialized derivation's triples. R(G) of SCHEMA-APG.md 17.1 leaves derived names
    out, so the oracle comparison and the instance corpus compare the view with derived names left out, as they leave
    out `asserted` and `live`.
K-8 Section 14: `@access` is recorded from stage 6.6 and read by stage 7's classifier; it waits for nothing and raises
    no UGS0902.

## Decided by the user
O-1 Materialized path derivations are stage 6.6's, walked from compiled steps; `Decimal` takes an exact sortable
    tokenizer (design decision 16).

## Further readings to write in (the corpus's, where it took one)
R-1 `DateTime − DateTime`, `Date − Date` and `LocalDateTime − LocalDateTime` give a `Duration` (5.5/4).
R-2 `orphan` needs an `Optional` single-valued role; on a set-valued role it is UGS0501 (6.7, 10.4's row).
R-3 `@limit`'s grammar takes an aggregate word and arguments; one other than `count()` or `sum(attr)` is UGS0505 at phase 5.
R-4 Attribute identity (10.2) holds over 6.3: a subtype's redeclaration may tighten requiredness and value checks only;
    one adding `@index` or `@immutable` is UGS0215; one leaving out an inherited `@immutable` removes it, UGS0518.
    An identical redeclaration is allowed and changes nothing.
R-5 A specialized role keeping neither its super role's optionality nor set-valuedness is UGS0407 (6.4, 645).
R-6 Restating the root's `@set` or `@multi` on a subtype is allowed; changing it is UGS0409.
R-7 `orphan` on an `@append_only` relation's role is UGS0601, as `cascade` is (8.3).
R-8 A composite key's claim is on the key as a whole; only a single-attribute key's predicate carries the `unique`
    flag (6.10's implicit row, 12.1).
R-9 A schema without its `prefix` header is refused with the code 6.1's header rule takes (UGS0006 if the grammar
    requires it); a module takes none.
R-10 6.6 lists `Duration` as range-indexable; 6.10 gives it a tokenizer (`int`, its nanoseconds) or the row drops it.

## From the second instance agent
G-1 8.2's `@acyclic` row: also at an update of the valid interval under `open` (an attribute the rule reads, as P1).
G-2 `max_depth` counts relation instances (edges) along a path.
G-3 `rule_read_max` counts the relation instances a check visits, each once: the walk's for `@acyclic`, the group's
    live instances for `@no_overlap`; a check that would visit more is refused before it does (8.2, 12.4).
G-4 The ugs-enforcement scenario "A walk past its bound" (change spec, ~line 341) uses aviation's `PartOf` by fan-out:
    a part holding 4,097 parts in one partition, made a part of another, refused with UGS1040 naming `@acyclic` and
    4,096 (`max_depth` 16 cannot bound a fan-out).
G-5 Section 14: a `@limit` per a role other than the anchor is enforced at stage 6.6 where the group's instances are in
    the transaction's partition, refused with code 23 otherwise, as every cross-partition rule is.
G-6 A derived name's or a navigation name's predicate counts as declared (7.4), so a client write of it is UGS1029,
    not UGS1001.
G-7 A client writes a `@bitemporal` or `@multi` edge-encoded instance with its reifier and the reifier's concrete type;
    the bare triple alone is refused with UGS1034's sibling rule (state which).
G-8 Design decision 3 records that the subset cannot create a keyless entity whose lower bound needs a relation in the
    same transaction (a Gen5 `Reading` with its `GeneratedBy`): no `let` or `returning`; quads carry it.
G-9 The instance README states: an operation's `iri` may be a blank label from an earlier step, and labels inside a
    triple term are substituted too.
