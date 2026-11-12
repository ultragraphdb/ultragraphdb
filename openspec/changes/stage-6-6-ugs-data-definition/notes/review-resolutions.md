# Review of the stage 6.6 change: findings and resolutions

* Status: R1 to R7 accepted by the user on 2026-11-10 and applied the same day, to the specification files and to this
  change. R3's file, `specification/schema/CATALOG.md`, is written (task 0.12); its six rules stand in `SCHEMA.md` section
  10.2. Written, rule 5 covers the expressions a commit evaluates: a function's body and a path derivation's are
  queries the stage 7 core plans, and format 1 stores them as text.
* Method: the elements of thought and the intellectual standards of Paul and Elder's *Miniature Guide to Critical
  Thinking* (purpose, question, information, inference, concepts, assumptions, implications, point of view; clarity,
  accuracy, precision, relevance, depth, breadth, logic, significance, fairness, completeness), applied by the author
  to the change as a whole and by four independent reviewers to the eighteen capability specifications.
* Part 1 holds the findings of the change-level review. Part 2 takes the reviewers' findings as they report.

## How the tenets decide

Each resolution is chosen by `AGENTS.md`'s order: correctness, speed, scale, developer experience.

| Tenet | What it asks of a resolution here |
|---|---|
| Ultra Safe | Fail closed: what cannot be decided or verified is refused, never passed. Every stored or received byte sequence has one valid reading, checked when it is read, and a component that finds its own state wrong stops |
| Ultra Speed | The cost is settled in the design: what a commit touches is laid out for it beforehand, read without parsing, allocation or recursion |
| Ultra Scale | One mechanism, specified once and read many times; a format or rule that later stages and other languages' clients must also read is written down before the first implementation, not derived from it |
| The order | Where two tenets pull apart, correctness decides, then speed, then scale, then the developer's convenience |

## Part 1. Findings of the change-level review

### R1. Only tenant 0 exists in stage 6.6 (a false assumption)

- **Finding.** The `metadata-group` specification creates tenant 0 at the first start and leaves every further tenant to
  stage 7's administration commands. The change assumes fresh and several tenants: the success criteria and tasks 12.1,
  12.4 and 12.5, design decision 2's consequence, and the scenarios of `ugs-catalog` and `uql-crud` that write into
  "another tenant". With one Install per tenant, none of them can run through the binaries.
- **Options.** (a) Keep one tenant and make the loop explicit; (b) bring a tenant-creating command into stage 6.6.
- **Resolution: (a).** Through the binaries a schema lives in tenant 0. A fresh schema is a fresh cluster, or a Drop of
  everything in the tenant followed by an Install, which is the stage's stated way to change a schema and so must be
  tested as one. Isolation between tenants holding different schemas is tested where tenants can be made, in
  `ultragraphdb_cluster_metadata` and `ultragraphdb_cluster_partition`, by the group's own command, as stage 5.5's
  scenarios of tenants 3, 7 and 8 are.
- **By the tenets.** Safe: a tenant-creating command without stage 7's grants would be an administration action any
  authenticated principal may take, which the gate's fail-closed design exists to prevent. Scale: the command's shape
  belongs to the administration design it is part of. The order: nothing of correctness is lost, since isolation is
  still tested, deterministically, at the layer that implements it.
- **Changes.**
  - Proposal, success criterion 1 and design decision 2: "a fresh tenant" becomes tenant 0 of a fresh cluster, and the
    loop is Drop of kind 4, then Install.
  - Tasks 12.1 to 12.5: each battery starts its own cluster; aviation after Gen5 by Drop and Install in one battery;
    the two-tenant isolation moves to a scenario of `ultragraphdb_cluster_partition`.
  - `uql-crud`, "A UQL write and its quads are one write": the set is written through UQL, the view read, the tenant
    dropped and installed again, the set written as quads, and the two views compared. The test then covers the loop.
  - A new scenario of `ugs-catalog`: Install, write, Drop of kind 4, Install of another schema, with no residue read.

### R2. A Drop of everything meets history that is "never physically discarded" (found resolving R1)

- **Finding.** `SCHEMA.md` section 8.4 keeps instances of `@bitemporal` and `@append_only` types until a legal purge.
  `PROTOCOL.md` section 6.5's Drop of a graph, of a tenant's data or of everything removes them. R1 makes that Drop the
  ordinary way to change a schema, on Gen5, whose every type keeps history.
- **Resolution.** The two rules are about different things, and `SCHEMA.md` says so: no *instance* operation, a delete
  or a retraction, discards such an instance; a Drop removes a *container*, a graph or a tenant's data, with everything
  in it, under the action `drop`, and is an audit event as a purge is. A Drop of kind 3 or 4 in a tenant that holds a
  schema names the tenant's catalog version in its request and is refused when that version is not the current one.
- **By the tenets.** Safe: the one irreversible operation on history is explicit, separately privileged and recorded,
  and the version it names keeps a Drop aimed at yesterday's schema from destroying today's data. The order:
  correctness of the retention rule is kept by stating its scope, not by an exception to it.
- **Changes.** `SCHEMA.md` sections 8.4 and 8.8; `PROTOCOL.md` section 6.5 (the version field and its refusal, code 2,
  kind 5); `SECURITY.md`'s audit events; a scenario of `schema-changes`.

### R3. The catalog version's bytes are specified nowhere (depth, completeness)

- **Finding.** `SCHEMA.md` section 10.2 lists what each entry holds and says a version has "one canonical encoding".
  No file gives the layout. Yet Describe sends each entry's bytes to clients, the schema id hashes them, every
  partition decodes them on its commit path, and task 4.3 asks for expected bytes "from the corpus", which cannot be
  written independently of the encoder without a layout to write them from.
- **Resolution.** A specification file of its own, `specification/schema/CATALOG.md`, written before group 4, as
  `PROTOCOL.md` leads the wire's code. Its decisions:
  1. **The frame's discipline, reused.** A version is a body in `PROTOCOL.md` section 4.1's layout: a fixed part, a
     section table, 8-aligned sections, zero padding, u32 indices into a string list. Its entry table is Describe's
     section 7 record (kind, name, offset, length), so Describe carries a version's entries without re-encoding.
  2. **Each entry self-contained.** An entry's bytes are again a section 4.1 body with a fixed part per kind and its own
     strings, since Describe carries any subset of entries to a principal and the schema id hashes them as stored.
  3. **Verified when read.** The fixed part holds a format version and an XXH3-64 of the bytes after it. A decoder
     checks the checksum, then every rule of the one encoding, order, offsets, zero bytes, interning order, and refuses
     anything else. A partition that cannot decode the version its log names stops, as `schema-storage`'s "A catalog
     entry that does not read back is corruption" already has it.
  4. **Laid out for the commit.** Beside the canonical, name-ordered entries the version holds an index from type prefix
     to Type entry, so a partition reaches a type's fields, roles, checks and prefix ranges by index. The decoded form
     of design decision 11 is then a set of views over the bytes, built once, with nothing parsed per commit.
  5. **Expressions as verified programs.** A `@check`, a `@limit` condition, a computed derivation and a function body
     are stored as typed postfix programs of fixed-width instructions over the entry's fields and constants, not as
     source text and not as a tree. The decoder verifies each program's stack discipline, operand bounds and depth, at
     most the grammar's nesting bound of 64; evaluation is then a loop with no recursion and no allocation. A partition
     needs no parser, so the compiler stays out of the storage role.
  6. **Independent expectations.** `CATALOG.md` holds a worked example encoded by hand, a schema of one entity with one
     attribute, as `PROTOCOL.md` section 2.4 works `hello.frame`: that fixes the layout independently of the code. The
     aviation and Gen5 versions are checked entry by entry and field by field against expectations written by hand
     from `SCHEMA.md`, through the decoder; their bytes are pinned as golden files once those checks pass.
- **By the tenets.** Safe: one valid reading, checked on every load, and programs proved well-formed before the first
  commit runs one. Speed: the layout a commit needs is decided now, when it is free, in the frame's own zero-copy
  form. Scale: the bytes are read by every role, by stage 9.5's evolution and by clients in other languages, so they
  are written down once; the wire's reading code and rules are reused, not rebuilt.
- **Changes.** A task before group 4 writing `CATALOG.md`; `SCHEMA.md` section 10.2 and `PROTOCOL.md` sections 8.3 and
  13.7 naming it; `ugs-catalog`, "A catalog version has one encoding", gaining the checksum, the prefix index and the
  program verification with their scenarios; task 4.3 reworded to point 6; design decision 11 restated as views.

### R4. The compilation report has no stated form (precision)

- **Finding.** Install's section 2 is "the report, UTF-8". `SCHEMA.md` section 9.5 lists what it covers and no form.
  Scenarios compare a report with the corpus's expectation for equality.
- **Resolution.** The report is data. Its content is the catalog version's Classification and Report entries, whose
  records `CATALOG.md` lays out: per write operation its path and deciding constraint, each scatter read, each
  placement conflict with what stayed local, each unenforced construct with its stage, the index entries per
  operation. The conformance corpus states and compares those records. Install's section 2 is their rendering, one
  line per record in the entries' canonical order, in a grammar `SCHEMA.md` section 9.5 fixes, so the text is
  deterministic and a client need not parse it: Describe gives it the records.
- **By the tenets.** Safe: what the tests compare is typed and exact. Scale: one source of the report, the catalog,
  rendered rather than restated. Developer experience comes last and is still served: the rendering reads as section
  10.4's diagnostics do.
- **Changes.** `SCHEMA.md` section 9.5; `CATALOG.md` (R3); `ugs-semantics`, "The compilation report", and the corpus
  task comparing records.

### R5. How an expression evaluates over an absent value is unstated (clarity)

- **Finding.** `QUERY-ULTRA-QL.md` section 5.2 makes UQL's logic two-valued, a comparison with an absent operand
  false. `SCHEMA.md` shares the expression grammar and never says how `@check`, `@limit` or a derivation evaluates.
  Gen5 depends on the answer: `@check(value is none or quantity_kind in value.unit.quantities)`, and `confidence:
  Float64 Optional @range(0, 1)`, which must admit an instance without a confidence.
- **Resolution.** `SCHEMA.md` section 4 states the evaluation of the shared grammar once, and UQL's section 5.2 cites
  it:
  1. Logic is two-valued. A comparison with an absent operand is false; arithmetic with one is absent; `is some` and
     `is none` test presence.
  2. A check on an attribute or a record field, `@pattern`, `@range`, `@values` and an attribute's `@check`, is a check
     of its value and holds vacuously of an absent optional value. A `@check` on a type is evaluated on the instance as
     written, so its author guards an optional field with `is none or`, as Gen5 does.
  3. Evaluation fails closed. A check or a limit whose evaluation overflows or divides by zero refuses the commit
     (`UGS1010`, the message naming the annotation and the fault); a computed derivation's refuses the write that
     would store it. No write passes on an expression that could not be evaluated.
  4. The compiler catches what it can before any data exists: a division by a literal zero is refused, under a new
     error `UGS0228`, and a type's `@check` that compares an optional field with no presence test warns, under a new
     warning `UGS0807`, since it refuses every instance without the value.
- **By the tenets.** Safe: fail closed, and the rule for absence is one sentence every reader and every engine shares.
  Speed: two-valued evaluation needs no third truth value in the program of R3. The order: the vacuous rule for
  attribute checks is the correct reading of an optional attribute, and it also happens to be the convenient one.
- **Changes.** `SCHEMA.md` sections 4, 5.2 and 6.6, and section 10.4's two new codes; `QUERY-ULTRA-QL.md` section 5.2;
  `ugs-enforcement`, "Value checks hold at every commit of the instance", with scenarios for an absent optional under
  `@range`, an unguarded type check, and an overflow.

### R6. `with retracted` and versions (a contradiction between specification files)

- **Finding.** `QUERY-ULTRA-QL.md` section 11.2 has `with retracted` read "every instance and version". `SCHEMA.md`
  section 8.4 has it read every instance, and `SCHEMA-APG.md` section 15.3 gives its slice each element's newest
  version.
- **Resolution.** `SCHEMA-APG.md` holds. A slice is an algebraic property graph, one value per element, and a query
  denotes a function of it; a read returning several versions of one element has no result type in UQL's section 5.
  `with retracted` reads every instance with its newest version; a past version is read by `believed at` or `as of`.
  `QUERY-ULTRA-QL.md` section 11.2 is corrected.
- **By the tenets.** Correctness: the formal model, which the Lean oracle checks, decides between two prose files.

### R7. The name of an edge-encoded instance's reifier (gap G, open since the operations were specified)

- **Finding.** `SCHEMA.md` section 7.3 names every relation instance without `@key` by a handle IRI of its type.
  `PROTOCOL.md` sections 9.1 and 10.2 name a statement's reifier by a skolem IRI, tagged as a blank node.
- **Resolution.** An instance has one name. The reifier of a declared relation's edge-encoded instance is the
  instance's identity, so it is named by the handle IRI of its type, `{base}{Type}/{handle}`, and presented as an IRI,
  tag `0x01`, in the reifier column. A skolem IRI names the reifier of a statement of no declared relation, in a tenant
  with no schema or under `@open`. Both state the bucket of the statement's subject in the same six digits, so routing
  is unchanged.
- **By the tenets.** Safe: one name per instance, the rule `UGS1038` already holds for subjects. Scale: one minting
  rule for every keyless instance. Developer experience: the IRI a client holds says what it names.
- **Changes.** `PROTOCOL.md` sections 4.4, 4.6, 9.1 (the reifier column admits tag `0x01` holding a handle IRI) and
  10.2; `SCHEMA.md` section 11.1; `mapping-contract` and `term-dictionary` deltas, which already assume it.

## Part 2. Findings of the independent reviewers

* Status: D1 to D19 accepted by the user on 2026-11-10, D10 by its option (a), and applied the same day with the
  corrections of part 2.2, to the specification files and to this change's specifications.
* The four reviews checked 85 requirements and 335 scenarios against `SCHEMA.md`, `PROTOCOL.md`,
  `QUERY-ULTRA-QL.md`, `SCHEMA-APG.md` and `deepbrain-gen5.ugs`, and reported 64 defects. Part 2.1 holds the nineteen
  that need a decision, because a specification file is silent, wrong or at odds with another; each is resolved in
  that file first. Part 2.2 holds the corrections to this change's own specifications, which need none.
* One reviewer's finding blocks the first success criterion as the files stand: D1.

### 2.1 Decisions

#### D1. Gen5 uses the reserved word `statement` as a name, fifteen times (blocks success criterion 1)

- **Finding.** `SCHEMA.md` section 3.4 reserves `statement`. Gen5 names a role or an attribute `statement` on fifteen
  lines, `AssertedBy.statement` first, so its Install is refused with `UGS0007`. The same section's rule that a name
  differing from a reserved word by case alone warns (`UGS0805`) fires on Gen5's `Statement`, `Relation` and
  `Quantity`, and on section 6.2's own example.
- **Resolution.** `statement` becomes a contextual keyword: the grammar has it in one place, after `abstract`, where
  one token of lookahead decides it, as `import`, `unit` and `version` were decided on 2026-11-08. The reserved words
  are nineteen. The case rule leaves `UGS0805`: a name in its kind's conventional case is deliberate. A keyword typed
  in the wrong case is caught where it happens, by the parser, whose `UGS0006` names the keyword as the fix.
- **By the tenets.** Correctness: the grammar stays unambiguous, and the specification stops warning on its own
  examples. Scale: one rule for every structural word that begins no declaration. Renaming fifteen uses in the schema
  of record would bend the domain's vocabulary to a parser convenience, which the order ranks last.
- **Changes.** `SCHEMA.md` sections 3.4, 4 and 10.4 (`UGS0006`, `UGS0805`), section 17; `ugs-syntax`.

#### D2. One condition, one diagnostic: precedence and duplicates

- **Finding.** Several conditions carry two codes with no rule between them: `UGS0116` and `UGS0710` (a graph over an
  unknown schema); `UGS0104` and `UGS0803`'s first clause; `UGS0114` and `UGS0701`; `UGS0411` and `UGS0516`;
  `UGS0101` against `UGS0201`, `UGS0112`, `UGS0113`, `UGS0117` and `UGS0405`; `UGS0006` against `UGS0120`; at commit,
  `UGS1015` against `UGS1034`'s "a type without its required roles". No rule says which refusal a commit reports when
  an instance breaks several, as a `NaN` on an attribute with `@range` does. Code 28's `detail` counts "diagnostics",
  which include warnings and notes.
- **Resolution.** `SCHEMA.md` sections 8.1 and 10.4 state the rule the refusal numbers were given on 2026-11-10: one
  condition, one number.
  1. The most specific code naming a condition is raised; a general one only where none does. `UGS0101` is for a name
     no specific code covers.
  2. `UGS0710` is retired, its condition `UGS0116`'s. `UGS0803` keeps a malformed local part and a relative IRI;
     `UGS0701` is `@colocate`'s and `UGS0114` every other annotation's; `UGS0516` is `restrict`'s and `UGS0411` a
     cardinality's and a `@limit`'s. `UGS1034` loses its required-roles clause to `UGS1015`.
  3. In an expression position a token the expression grammar does not expect is `UGS0120`; elsewhere `UGS0006`. An
     operation version 1 leaves out, measure arithmetic beyond section 5.8's, is `UGS0120` at typing.
  4. A commit reports one refusal: the first rule broken in the order of section 8.2's table, a value's type before any
     check that reads it, for the first instance in the order its changes arrived.
  5. Code 28's `detail` counts the diagnostics of kind error.
- **By the tenets.** Safe: a test and a client act on one exact number. Scale: the rule is stated once and decides
  every later pair.
- **Changes.** `SCHEMA.md` sections 8.1, 8.7 and 10.4; `PROTOCOL.md` section 9.4; `ugs-syntax`, `ugs-semantics`,
  `ugs-enforcement`, `ugs-catalog`.

#### D3. Three lexical rules with no code or an untestable form

- **Finding.** `inf`, `-inf` and `nan` are "refused as literals" with no code, and are legal identifiers. Text that
  is not UTF-8 has no code. A line's length does not say whether it counts its line break.
- **Resolution.** There is no literal for a non-finite number: the words are identifiers, and where a number is
  expected one is refused as any identifier naming no value of the type is (`UGS0207`). A byte sequence that is not
  UTF-8 is `UGS0001` at the line and column of its first invalid byte. A line's 64 KiB excludes its line break.
- **Changes.** `SCHEMA.md` sections 3.1, 3.5 and 10.4; `ugs-syntax`.

#### D4. The worked example of section 16 disagrees with the rules it works

- **Finding.** Section 9.1 makes every navigation of a role that is not the anchor a scatter read. Section 16 lists
  `Organization.agreements`, the anchor role's own, and omits `Organization.counterparty_agreements`,
  `Organization.leases_in`, `Aircraft.leases` and the derivation `Company.suppliers`. Its `UGS0903` names two of three
  cases, omitting `Supply`'s `restrict` on `buyer`. Sections 9.4 and 16 call `UGS0904` a note; the table makes it a
  warning.
- **Resolution.** The example is an oracle, so it is made exact from the rules: the scatter reads are `assets`,
  `parts`, `counterparty_agreements`, `purchases`, `contracts`, `leases_in`, `leases` and `certifications`, with the
  derivations `all_parts` and `suppliers` that begin with one; `UGS0903` names `PartOf.whole`, `Supply.item` and
  `Supply.buyer`; `UGS0904` is a warning everywhere.
- **Changes.** `SCHEMA.md` sections 9.4 and 16; `ugs-semantics`' two aviation scenarios, stated as exact lists.

#### D5. What `UGS0902` and `UGS0806` count

- **Finding.** `UGS0902` is listed for things no schema declares, the views, cross-partition paths and Purge, and
  nothing says whether Gen5's index on a predicate three types share is one record or three. `UGS0806` warns on "a new
  reference in a later source", which one Install per tenant leaves undefined.
- **Resolution.** `UGS0902` is one record per declaration that waits: each `@index(ann)` declaration, each function,
  each virtual derivation. Gen5 has twenty: one, fourteen and five. What a stage refuses at run time is no record.
  `UGS0806` is a reference to a `@deprecated` item from another source than the one declaring it; stage 9.5 adds a
  reference new in a later version.
- **Changes.** `SCHEMA.md` sections 6.7, 9.5 and 14; `ugs-semantics`.

#### D6. Three readings Gen5's compilation depends on

- **Finding.** `Alternatives`' `@check(all members(higher = source))` reads a participant's roles, and section 6.6
  admits only a participant's "immutable fields and types". `Domain.prevailing` orders by `edge.authority_rank`, and
  `edge` is defined only inside a step's filter. `UGS0802` does not say whether `precedes` and `Precedes` collide.
- **Resolution.** A participant's required, single-valued role is an immutable field, by `UGS1025`. In a function's
  `order by`, `edge` is the instance the body's last step traverses. `UGS0802` compares names byte for byte, as the
  GQL core's identifiers are case-sensitive.
- **Changes.** `SCHEMA.md` sections 5.11, 6.6, 6.9 and 7.4.

#### D7. Blank nodes: one request, and where a new relation instance binds

- **Finding.** `PROTOCOL.md` section 4.4 scopes a blank node's label to one request, so an instance named by one
  cannot be written across several Writes, as two scenarios do. Section 11.12 binds a transaction that writes only new
  blank nodes to a partition in turn, while a new relation instance must live in its anchor's bucket.
- **Resolution.** A keyless instance created under a blank node is created whole in one request; a later request
  names it by its handle, which a read in the transaction answers. `PROTOCOL.md` section 11.12's binding table gains
  a row: a new blank node typed as an unkeyed, node-encoded relation binds its transaction to the partition owning the
  bucket of its anchor participant, the object of its anchor role's triple.
- **By the tenets.** Correctness: an instance and its anchor in one partition is the placement invariant every local
  check rests on, so it cannot depend on the order partitions are taken in.
- **Changes.** `SCHEMA.md` section 11.4; `PROTOCOL.md` sections 4.4 and 11.12; `ugs-views`, `ugs-enforcement`.

#### D8. A keyed instance written under a blank node

- **Finding.** `SCHEMA.md` section 7.3, since 2026-11-10, creates a keyed instance under the IRI its key mints and no
  other. `QUERY-SPARQL-CORE.md`, in its section on writes, lets a client write a blank node with the key attribute and "let the engine
  mint".
- **Resolution.** `SCHEMA.md` holds, and the sentence leaves the SPARQL core: a keyed instance under a blank node is
  refused (`UGS1038`). The IRI is a pure function of the base, the key space and the key, which every client library
  computes; the frontend routes by a subject's name alone and assembles no instance to find a key.
- **By the tenets.** Safe: one name per instance and one spelling per write. Speed: routing reads one term.
  Developer experience pays one function call, and is last in the order.
- **Changes.** `QUERY-SPARQL-CORE.md`'s section on writes; `ugs-identity`.

#### D9. What makes a write a creation, and `UGS1004` against `UGS1008`

- **Finding.** A `Person` created with the `id` an existing `Company` holds writes on the same IRI, since both mint
  from `Node`. It is "another type given to an existing instance" (`UGS1004`) and "a key another instance of the tree
  holds" (`UGS1008`) at once, and no file says what a creation is.
- **Resolution.** A transaction creates an instance when it writes `rdf:type` on a blank node or on a keyed type's
  minted IRI. A creation whose key an instance of the tree holds is `UGS1008`, whatever its concrete type, and is
  never read as an update: instances are not merged, as a migration never merges them. `UGS1004` is two types in one
  creation, or a type written on an existing keyless instance's handle.
- **Changes.** `SCHEMA.md` sections 8.1 and 8.7; `ugs-enforcement`.

#### D10. A line string on an indexed geometry

- **Finding.** `SCHEMA.md` section 5.7 admits a `LineString`. The store's geo tokenizer covers points and polygons and
  refuses every other geometry. Gen5 indexes three `Geo` attributes.
- **Options.** (a) the tokenizer gains a cover for a line string; (b) a `LineString` is refused on an attribute with
  `@index(geo)` until it does.
- **Resolution: (a).** The value is admitted by the type and the index is a stage 6.6 row, so the index covers it: an S2
  cover of the polyline at the levels the polygon cover uses. Whichever is built, no value is stored that its index
  does not hold, since a read through the index would then answer wrongly.
- **Changes.** `tokenizers` (a MODIFIED "Geo tokens index S2 cells"); `ugs-enforcement`.

#### D11. The `measure` tokenizer's claim

- **Finding.** The delta says measure tokens order as their canonical values do. The `float` tokenizer it builds on
  truncates to an integer, so tokens are non-decreasing and lossy, and a range filter compares the values.
- **Resolution.** The delta states exactly that, with the tokenizer's identifier byte, the registry's next free one,
  and its facts: sortable, lossy, valid for `ugdb:measure`.
- **Changes.** `tokenizers`.

#### D12. Roles and attributes holding a catalog type

- **Finding.** The participant checks, existence, liveness and type, are stated for every role. A role naming
  `schema.RelationType<T>` holds a value, as Gen5's `Absence.kind` does, and no number refuses a type outside `T`.
- **Resolution.** Such a role takes no existence or liveness check. A type outside `T` breaks the rule its position
  already has: `UGS1018` in a role, `UGS1011` in an attribute.
- **Changes.** `SCHEMA.md` sections 5.10 and 8.2; `ugs-enforcement`.

#### D13. An expression that cannot be evaluated has its own number

- **Finding.** Section 4, since 2026-11-10, refuses a check that overflows with `UGS1010`, the number of a check that
  evaluated to false, and gives a computed derivation's failure no number.
- **Resolution.** By D2's rule the condition takes one number of its own: `UGS1039`, EvaluationFailed, for a `@check`,
  a `@limit` condition or a computed derivation whose evaluation overflows or divides by zero. `UGS1010` stays a check
  that evaluated and did not hold.
- **Changes.** `SCHEMA.md` sections 4 and 8.7; `ugs-enforcement`.

#### D14. Four stale or conflicting sentences in `SCHEMA.md`

- **Finding and resolution.**
  - Section 6.4's role table says a relation type named by a role "is then node-encoded"; section 11.1 and the
    decision of 2026-11-05 keep it edge-encoded, filling the role through its reifier. Section 6.4 is corrected.
  - Section 7.2 gives `Text` a canonical key form; section 6.3 does not make `Text` keyable. `Text` leaves 7.2.
  - Section 7.4 gives a graph's IRI as "`{base}{graph}`, or its `@base`"; section 6.11 makes `@base` the instance
    base. The graph's IRI is `{base}{graph}`.
  - Section 12.2 stores a precision "as an `xsd:string`"; sections 5.1 and `PROTOCOL.md` 10.4 give it the datatype
    `ugdb:precision`, which holds.
  - Section 14's stage 6.6 row lists `@retracts` and not `@ends`, which section 9.3 places with it. `@ends` joins the
    row.
- **Changes.** `SCHEMA.md` sections 6.4, 7.2, 7.4, 12.2 and 14.

#### D15. The dictionary when an entity is deleted and created again

- **Finding.** The `term-dictionary` requirement "The dictionary is a keyspace without versions" maps an IRI to a
  `Resource` ID, removes no entry "in this stage", and leaves a delete to stage 6.6. The change deletes entities and
  lets a deleted key be created again with a new ID. A read at an earlier version that names the IRI would then
  resolve the new ID and answer that nothing existed.
- **Resolution.** A delete removes no dictionary entry: the ID's entry names its IRI for good, and the tombstone
  under the ID holds the version of the delete. A creation on the IRI rewrites the IRI's entry to the new ID, whose
  record names the ID it replaced. A read naming the IRI at a version below that delete resolves the replaced ID. The
  chain is as long as the IRI's deletes within the retention window and is trimmed with it.
- **By the tenets.** Safe: a read never answers absence where data existed. Speed: the dictionary stays without
  versions, and the extra step is taken only by a read of the past through a recreated IRI.
- **Changes.** `term-dictionary` (a MODIFIED requirement); `PROTOCOL.md` section 12.2; `SCHEMA.md` section 8.5.

#### D16. The order of sources in one compilation

- **Finding.** Prefixes follow "schemas before the modules extending them and each source in text order". Two schemas
  in one Install, or two modules over one schema, have no order, and prefixes are permanent.
- **Resolution.** The order is a function of the sources alone, never of the request: schemas by name as UTF-8 bytes,
  then modules, each after what it extends, ties by name. Recompiling the stored sources then allocates alike.
- **Changes.** `SCHEMA.md` section 7.1; `ugs-identity`.

#### D17. The case of a percent-encoding

- **Finding.** `%3a` and `%3A` are two IRI texts, and `UGS1038` refuses any IRI but the minted one.
- **Resolution.** A minted IRI writes its percent-encodings in uppercase hexadecimal digits, as RFC 3986 recommends.
- **Changes.** `SCHEMA.md` section 7.3; `ugs-identity`.

#### D18. What a one-frame write may carry (corrects recommendation C of 2026-11-10)

- **Finding.** `SCHEMA.md` section 9.6, as amended on 2026-11-10, sends to a transaction every change "that a check or
  a bound reads". Gen5's `Person` has a `@check` reading `description`, so no DirectWrite could set a description on
  any node. Section 9.6's own definition of blind is narrower: no check that reads *beyond the instance*.
- **Resolution.** The definition holds. A check over the instance's own fields is validated in the frame's one turn,
  as a datatype is, and refuses the frame with its number. What takes a transaction is a claim of a key or a `@unique`
  value and a rule reading beyond the instance: a bound, a `@limit`, `@no_overlap`, `@acyclic`, a participant's field.
  The catalog carries, per type, whether creating an instance of it is blind.
- **By the tenets.** Correctness: the amended sentence contradicted the definition beside it. Speed: an update of one
  attribute stays one frame.
- **Changes.** `SCHEMA.md` section 9.6; `PROTOCOL.md` sections 6.6 and 6.7; `ugs-catalog`.

#### D19. The UQL subset's edges

- **Finding.** The subset leaves open: a derived name in a shape (a scenario reads `closed`); Query's header `limit`
  and resume key; the nesting bound `UQL0001`; a parameter whose term does not fit its type; the order of a `Set` in
  an `rdf:JSON` cell.
- **Resolution.** A shape holds attributes and the four built-in fields; a derived name, and so `closed`, is refused
  with `UQL0901`, and `..` leaves derived names out. The lesser of the header's `limit` and the text's applies; the
  subset takes no resume key, and one sent is refused. Nesting past 64 is `UQL0001`. A parameter's term that does not
  fit its inferred type is code 11, `detail` 0. A set's members are written in the ascending order of their term
  encodings, so one set has one text.
- **Changes.** `QUERY-ULTRA-QL.md` section 4.1; `SCHEMA.md` section 11.2; `uql-crud`.

### 2.2 Corrections to this change's specifications

Each is an error or an omission against a source that is itself sound, and takes the wording the source gives.

| Capability | Correction |
|---|---|
| `ugs-syntax` | The corpus covers `UGS0001` to `UGS0011`, `UGS0013`, `UGS0120`, `UGS0403` and the warnings `UGS0804` and `UGS0805`; there is no `UGS0012`. A `///` run documents the `schema` or `module` line, never an `import`, `prefix` or `reserve`. The 65-parenthesis scenario sits in a `derive` body. The bounds of section 3.7 on a union, an enum, a key, a derivation path and a function's parameters are added. "A navigation name without a cardinality marker" replaces "a role without". The `prefix` base rule (`UGS0009`) is added |
| `ugs-semantics` | The corpus asks, for a warning or a note, a valid source listing it beside `UGS0901`. `UGS0201` is an unknown type name. `UGS0308` and the bounds on a type's fields, a relation's roles and a compilation's sources are added. The symmetric scenario is split in two |
| `ugs-identity` | Bucket literals are symbolic or in tenant 0's run, `c7…`. "A tree declared contiguously is one range". The vocabulary gains an enum's and a derived name's IRIs, and `{Relation}` is the type declaring the role. A `Reading` under a blank node commits with its `valid`, a value and its `GeneratedBy`. An unheld handle is refused at the Write, by the term's rule; `UGS1038` is for an IRI of no handle form |
| `ugs-views` | A keyed relation instance's entries are in its key's partition. The default read adds what a `@retracts` role reaches, with a `Supersedes` scenario. A reifier fills a role as subject or object, with both scenarios. "An insert or a delete of a derived property". The read of a new `MemberOf` lists what definition 17.1 gives, defaults and supertypes among it |
| `ugs-enforcement` | A preamble: one partition holds the tenant's run unless a scenario names partitions. The `Reading` scenarios state `valid` and a `GeneratedBy`. "Replaces, deleting the present value and inserting another" in five scenarios. A relation over a relation uses `DependsOn`, an edge. `UGS1016` at a removal, a relation instance's delete, and scenarios for `UGS1002`, `UGS1006` and `UGS1031` are added. A `Vector<BF16>` write's `detail` is the term's index. Subjects are named `Document`, "as `collection`", "states an end"; the pattern scenario uses `xORG:acme`; aviation is named where Gen5 declares the same type |
| `scalar-types` | A malformed measure is stated case by case; a unit code past 15 bytes is encoded by its lexical form; a `ugdb:bfloat16vector` scenario is added |
| `ugs-catalog` | The fence is raised after the checks of `version` and of the tenant's schema, with `detail` 0, ending a transaction, reads going on. Each refusal states the `detail` the protocol gives. An Operation of kind 5 or 6 naming an IRI no instance holds, or in a tenant with no schema, is refused. A source with an error names its construct and code. The continuation scenarios use a schema built past a frame. "Describe answers both sources" |
| `uql-crud` | `UQL0507` holds of a role binding in a query. The `Post` scenario states that the `Company` exists |
| `wire-protocol` | MODIFIED "Result batches are columnar and dictionary-encoded": twelve sections, Prepare ten. MODIFIED "Roles exchange role commands": a Reference's `outcome` 30. Query takes `language` 1 to 3, Prepare 0 to 3. Describe's declarations in sections 5 and 6, its entries in 7 and 8. Describe and Catalog frames spend credit. The write-set record's operations 8 and 9 |
| `client-endpoint` | The gate lists Purge and Migrate, with `purge` and `declare` |
| `metadata-group` | The sequence scenario names the aviation schema |
| `system-identity`, `catalog` | MODIFIED "The bucket comes from the tenant and the key" and "Unique values": a `@key`'s claim in the instance's own bucket, a `@unique` value's at the value's. "System IDs never leave the database" and the computed-never-chosen sentence are scoped to what they still hold of |
| `mapping-contract`, `term-dictionary` | MODIFIED "Statements are the canonical model", "Names map between the views" and "Statements round-trip through both views", each opened "in a tenant with no schema" with the governed rule beside it. A handle's bucket is drawn for an entity and the anchor's for a relation. The skolem scenario names a tenant with no schema |
