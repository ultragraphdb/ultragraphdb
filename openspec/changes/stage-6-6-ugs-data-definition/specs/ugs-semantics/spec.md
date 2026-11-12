## ADDED Requirements

### Requirement: Compilation runs its phases over every source and refuses whole
The compiler SHALL run phases 2 to 7 of `SCHEMA.md` section 10.1, resolution, typing, identity, relations and
constraints, placement and classification, and views, over the full set of sources a catalog version holds, schemas
before the modules extending them. A diagnostic of kind error in any phase SHALL refuse the compilation whole, with
every diagnostic found up to the end of that phase and no catalog version. The compiler SHALL be a pure function of
its sources and the prefixes it is given: the same input SHALL give the same model, the same report and the same
bytes.

#### Scenario: One error refuses every source
- **WHEN** the aviation schema is compiled together with a second schema holding an inheritance cycle
- **THEN** the compilation is refused with `UGS0105`, and no catalog version is produced for either schema

#### Scenario: Compiling twice gives the same bytes
- **WHEN** `deepbrain-gen5.ugs` is compiled twice with the same prefixes
- **THEN** the two catalog versions are byte for byte equal, and so are the two reports

### Requirement: Names resolve
Every name SHALL resolve as `SCHEMA.md` sections 6.1 and 6.12 state: a type, attribute, role, enum member, unit,
quantity kind, function or prefix to its declaration in the source, in a schema it imports, qualified, or in what a
module extends, qualified or bare where unambiguous. The compiler SHALL refuse an unresolved name (`UGS0101`), a
duplicate declaration, a second `prefix`, a second `@default` graph or a clash with a built-in (`UGS0102`), an
`import schema` of a schema the tenant does not hold (`UGS0103`), a `@maps_to` prefix no `import` declares
(`UGS0104`), an inheritance cycle (`UGS0105`), a parent of the wrong kind or a root type in a module (`UGS0106`), a
version bound the tenant does not meet (`UGS0109`), a reserved name or the prefix `schema` declared or imported
(`UGS0110`), a name two modules declare (`UGS0111`), an undeclared enum member (`UGS0112`), an ill-formed quantity or
unit (`UGS0113`), an annotation naming a role or an attribute the type lacks (`UGS0114`, `UGS0115`), a graph over an
unknown schema (`UGS0116`), a call of an unknown function (`UGS0117`), a cycle in the function call graph
(`UGS0118`) and a body naming an unknown parameter or field (`UGS0119`). Quantity kinds and units SHALL each resolve
in a name space of their own. One condition SHALL take one code: the most specific
code naming a condition SHALL be raised, `UGS0101` only where no other names it, `UGS0701` in place of `UGS0114` for
`@colocate`, and `UGS0104` for a `@maps_to` prefix no `import` declares.

#### Scenario: A quantity kind may bear a scalar type's name
- **WHEN** Gen5 declares `quantity Time` and `quantity Duration` beside fields of type `Measure<Time>` and
  `Interval<DateTime>`
- **THEN** the schema resolves, `Measure<Time>` naming the quantity kind

#### Scenario: A module names its core's types
- **WHEN** `module net version 1 extends deep_brain >= 5` declares `entity Router : Object { }`
- **THEN** `Object` resolves to `deep_brain.Object`, and `Router` is an entity type below it

#### Scenario: A module's version bound is checked
- **WHEN** `module net version 1 extends deep_brain >= 6` is compiled with `deep_brain` at version 5
- **THEN** it is refused with `UGS0109` naming `deep_brain`, 6 and 5

#### Scenario: A subtype's field needs a narrowing filter
- **WHEN** a function body reads `all_parts(thrust_kn > 300)` where `all_parts` yields `Asset`
- **THEN** it is refused with `UGS0119`, the fix naming `is Engine`

### Requirement: Types and values are checked
Every type SHALL be one of `SCHEMA.md` section 5's, with its arguments in range: `Decimal(p, s)` within its bounds
(`UGS0203`), a vector dimension of 1 to 65,535 (`UGS0204`), a collection or record element the type allows
(`UGS0205`), an `Interval` over `Date`, `DateTime`, `LocalDateTime`, an integer type or a `Decimal` (`UGS0206`), a `Measure` whose unit is of its kind
(`UGS0218`), a type name that is declared or built in (`UGS0201`), and type arguments of the right count and kind
(`UGS0202`). A literal, default, derivation
or function body SHALL have the type its position expects (`UGS0207`); a default on an `Optional` field SHALL be
refused (`UGS0209`). An annotation SHALL fit its field's type: `@pattern` (`UGS0211`), `@range` (`UGS0212`),
`@values` (`UGS0528`) and each index kind (`UGS0522` to `UGS0526`). An inherited attribute SHALL be redeclared only
to tighten it (`UGS0213`, `UGS0214`), and one attribute name SHALL be one predicate per vocabulary base, declared
alike by every type declaring it unless `@iri` splits it (`UGS0215`), with a `@key` or `@unique` predicate declared
by one type tree (`UGS0216`). Enum members and record fields SHALL be unique (`UGS0221`, `UGS0222`). `@unique` SHALL stand on a scalar, enum or `Iri`
attribute (`UGS0308`). A type's fields, own and inherited, SHALL be at most 4,096, a relation's roles 64 and a
compilation's sources 1,024 (`UGS0013`). An identifier naming no value of the expected type, `inf` and `nan` where a
number is expected among them, SHALL be refused with `UGS0207`, and measure arithmetic beyond addition and subtraction
within one kind and scaling by a number with `UGS0120`. A participant's required, single-valued roles SHALL count
among its immutable fields. Expressions SHALL
be typed against their context: temporal kinds never mixed (`UGS0223`, `UGS0224`), no `today()` or `now()` in a
check, limit, derivation or function (`UGS0225`), a `@check` reading only the instance's own fields and its
participants' immutable fields and types (`UGS0226`), a computed derivation reading the instance alone
(`UGS0227`), and no division by a literal zero (`UGS0228`); a type's `@check` comparing an optional field with no test
of its presence SHALL warn (`UGS0807`). Each check, limit condition and computed derivation SHALL be compiled to
the postfix program the catalog version stores; a function's body and a path derivation's SHALL be stored as their
text.

#### Scenario: Gen5's shared attribute names agree
- **WHEN** Gen5 declares `uri: Iri Optional @index(range)` on `Object`, `Event` and `Information`
- **THEN** the three share one predicate `db:uri`, and the schema compiles

#### Scenario: A shared attribute name declared differently is refused
- **WHEN** one type declares `status: Status` and another `status: String` under one vocabulary base
- **THEN** the schema is refused with `UGS0215` naming `status` and both types

#### Scenario: An attribute with its own predicate stands outside another tree's claim
- **WHEN** Gen5 declares `action_identifier: String @unique` on `Action` and `action_identifier: String @index(range)
  @iri("https://example.org/deep_brain#teloid_action_identifier")` on `Teloid`
- **THEN** the schema compiles with two predicates, and only `Action`'s carries the unique flag

#### Scenario: A subtype tightens an inherited attribute
- **WHEN** `Reading : Statement` redeclares the optional `valid: Interval<DateTime>` as required with the same index
- **THEN** the schema compiles, and `valid` is required on `Reading`

#### Scenario: A subtype loosening an attribute is refused
- **WHEN** a subtype redeclares a required inherited attribute as `Optional`
- **THEN** the schema is refused with `UGS0214` at the redeclaration

#### Scenario: A default of another type is refused
- **WHEN** a field declares `rank: Rank = "normal"`
- **THEN** the schema is refused with `UGS0207`, the fix writing the member bare as `normal`

#### Scenario: A check reading a mutable field of a participant is refused
- **WHEN** a relation declares `@check(owner.status = active)` and `status` is not `@immutable`
- **THEN** the schema is refused with `UGS0226` at `owner.status`

#### Scenario: A check reading a participant's immutable field compiles
- **WHEN** Gen5 declares `relation ClassifiedAs : Relation @check(instance is kind.supertype)` and `supertype` is
  `@immutable`
- **THEN** the schema compiles

#### Scenario: A check reads a participant's required roles
- **WHEN** Gen5 declares `relation Alternatives : Asserted @multi @check(all members(higher = source))`, `members`
  naming `DerivesFrom`, whose role `higher` is required
- **THEN** the schema compiles

#### Scenario: A function orders by the edge its last step traverses
- **WHEN** Gen5 declares `fn Domain.prevailing() -> Source* = authorities order by edge.authority_rank desc`
- **THEN** the body type-checks, `edge` being the relation instance `authorities` traverses

#### Scenario: A division by a literal zero is refused
- **WHEN** a type declares `@check(value / 0 > 1)`
- **THEN** the schema is refused with `UGS0228` at the division

#### Scenario: An unguarded check on an optional field warns
- **WHEN** a type declares `notes: Text Optional` and `@check(notes <> "")`
- **THEN** the schema compiles with one warning `UGS0807` naming `notes`, and `@check(notes is none or notes <> "")`
  compiles with none

### Requirement: Relations and their constraints are consistent
The compiler SHALL check every relation as `SCHEMA.md` sections 6.4, 6.5 and 6.7 state: at least two roles
(`UGS0401`); participant types that are declared entity, relation or statement types, with no repeated or nested
union member (`UGS0402`); role and navigation names unique where they land, built-in fields included (`UGS0404`,
`UGS0417`); specialization with `as` naming a super role once and narrowing its type (`UGS0405`, `UGS0406`,
`UGS0407`, `UGS0410`); `@symmetric` on a binary relation with one participant type and one navigation name
(`UGS0408`); `@set` or `@multi` fixed at the root (`UGS0409`); nothing a `_` side cannot check, a cardinality or a `@limit` (`UGS0411`), a `restrict` (`UGS0516`); catalog types as participants with `-> _` (`UGS0412`, `UGS0413`); an edge-encoded relation's anchor role
with a navigation name (`UGS0418`); and the constraint rules `UGS0501` to `UGS0519`, `UGS0529`, `UGS0601` and
`UGS0602`. A subtype SHALL tighten and never loosen what it inherits (`UGS0214`, `UGS0518`, `UGS0519`).

#### Scenario: A relation's roles specialize its supertype's
- **WHEN** Gen5 declares `relation MemberOf : Relation` with `member: Endurant | Gap as subject` and `collection:
  Object | Event | Organization as object`
- **THEN** the schema compiles, `member` specializes `subject`, `collection` specializes `object`, and each union
  member is `Statement` or a subtype of it

#### Scenario: A specialized participant outside the super role's type is refused
- **WHEN** a subrelation of `Agreement` declares `supplier: Asset as party` and `party` takes `Organization`
- **THEN** the schema is refused with `UGS0407` naming `Asset` and `Organization`

#### Scenario: An append-only relation that would lose instances is refused
- **WHEN** the aviation `Supply` is declared `@append_only` without its `@on_delete` and `Asset` is deletable
- **THEN** the schema is refused with `UGS0601`, the fix adding `restrict` on each role

#### Scenario: A bitemporal relation over a mutable participant is refused
- **WHEN** a `@bitemporal` relation names a participant type that is neither `@bitemporal` nor an `@append_only`
  entity type
- **THEN** the schema is refused with `UGS0602` naming the role and the type

#### Scenario: A lower bound over two roles is refused
- **WHEN** a relation declares `@limit(count() >= 1 per (a, b))`
- **THEN** the schema is refused with `UGS0529`

#### Scenario: A symmetric relation needs one participant type and one navigation name
- **WHEN** Gen5 declares `relation ConnectsTo : Relation @symmetric` with both roles `Statement` and both navigation
  names `connects`
- **THEN** the schema compiles

#### Scenario: A symmetric relation with two navigation names is refused
- **WHEN** a `@symmetric` relation declares `a: Statement -> x*` and `b: Statement -> y*`
- **THEN** the schema is refused with `UGS0408`

### Requirement: Placement and classification are derived from the declarations
The compiler SHALL place every instance as `SCHEMA.md` section 9.1 states: a keyed instance by its key, a keyless
entity by its handle, an unkeyed relation instance at its anchor participant, the role its type's `@colocate` names,
else the nearest supertype's, else the first declared role. It SHALL refuse `@colocate` naming a role the relation
neither declares nor inherits (`UGS0701`), `@colocate` on a keyed relation (`UGS0702`), and `@materialized` on a
derivation that leaves the anchor partition (`UGS0704`). It SHALL classify every write operation the catalog permits by the most demanding path among its
constraints' rows of section 9.3, and SHALL record, per predicate and per type, the request-typing facts of section
9.6, including for paths that execute from stage 8.

#### Scenario: The aviation anchors
- **WHEN** the aviation schema is compiled
- **THEN** `Owns` is anchored at `asset`, `PartOf` at `part`, `Supply` at `supplier`, `Lease` at `lessor` and
  `Certifies` at `contract`

#### Scenario: A subrelation's own colocation decides its anchor
- **WHEN** Gen5 declares `relation MemberOf : Relation @colocate(collection)` and `Relation`'s first role is `subject`
- **THEN** `MemberOf` is anchored at `collection`, every other subrelation without `@colocate` at its `subject` role,
  and the report holds the warning `UGS0904` for `MemberOf`'s inherited `@limit` per `(subject, type)`

#### Scenario: The aviation classification
- **WHEN** the aviation schema is compiled
- **THEN** the classification holds the rows of `SCHEMA.md` section 16's table: creating an `Engine` segmented and
  local, updating `status` coordination-free, inserting `Owns` segmented and local, inserting `Supply` coordinated

### Requirement: View names do not collide
The compiler SHALL mint every vocabulary IRI of `SCHEMA.md` section 7.4 and refuse two terms minting one IRI
(`UGS0801`), two names the GQL view cannot tell apart (`UGS0802`) and a malformed `@maps_to` target (`UGS0803`). A reference to a `@deprecated` item from another source than the one declaring it SHALL warn with `UGS0806`.

#### Scenario: A navigation name colliding with an attribute's IRI is refused
- **WHEN** a schema declares the attribute `owner` on one type and the navigation name `owner` on another under one
  base
- **THEN** it is refused with `UGS0801` naming the IRI `{base}owner` and both terms

#### Scenario: A deprecated unit still compiles and warns where newly used
- **WHEN** Gen5 declares `unit ft_us … @deprecated(use: ft_i)` and a module declares a field `Measure<Length, ft_us>`
- **THEN** the compilation succeeds, and its diagnostics hold one `UGS0806` at the module's field, naming `ft_us` and
  `ft_i`

### Requirement: The compilation report
Every successful compilation SHALL emit the report of `SCHEMA.md` section 9.5 as records of the catalog version's
Classification and Report entries: each write operation with its path, whether its deciding state is local, the
constraint deciding it and its index entries (`UGS0901`), each scatter read (`UGS0905`), each placement conflict with
what stayed local (`UGS0904`), each `restrict` whose coordination `@colocate` would avoid (`UGS0903`), each declared
construct this stage does not enforce with its stage (`UGS0902`), and the ten operations writing most index entries
(`UGS0906`). The report's text SHALL be the rendering of those records and nothing more: one line per record in the
entries' canonical order, the code, the subject's qualified name, then the fields as `name=value` pairs in the order
section 9.5 lists them, so equal records render to equal text. The conformance corpus SHALL state and compare
records.

#### Scenario: The aviation scatter reads
- **WHEN** the aviation schema is compiled
- **THEN** the report's `UGS0905` records are exactly `Organization.assets`, `Asset.parts`,
  `Organization.counterparty_agreements`, `Company.purchases`, `Asset.contracts`, `Organization.leases_in`,
  `Aircraft.leases`, `Authority.certifications`, and the derivations `Asset.all_parts` and `Company.suppliers`

#### Scenario: A restrict that colocation would make local
- **WHEN** the aviation schema is compiled
- **THEN** the report's `UGS0903` records are exactly `PartOf.whole`, `Supply.item` and `Supply.buyer`, each suggesting
  `@colocate` on that role

#### Scenario: A record renders to one line
- **WHEN** the aviation schema is compiled
- **THEN** the report's text holds the line `UGS0905 aviation.Organization.assets relation=aviation.Owns role=owner`, and the catalog
  version's Report entry holds the record it renders

### Requirement: A construct of a later stage is compiled, recorded and reported
A construct whose runtime rule `SCHEMA.md` section 14 assigns to a later stage SHALL be parsed, validated by every
static rule and recorded in the catalog version, and the report SHALL name it with `UGS0902` and the stage that
enforces or executes it: each `@index(ann)` declaration with its `metric` and `quantization` (stage 8), each function and
each virtual derivation (stage 7), one `UGS0902` record per declaration. What a stage refuses at run time, a rule
across partitions, SHALL be no record. A static rule of such a construct SHALL still refuse: `@index(ann)` on a field that is
not a `Vector`, with an unknown metric or quantization, or with a search-time parameter (`UGS0524`).

#### Scenario: Gen5's vector index is declared and unenforced
- **WHEN** Gen5's `attribute Embedding = Vector<BF16, 768> @index(ann, metric: cosine, quantization: int8)` is compiled
- **THEN** the compilation succeeds, the catalog version records the index with metric `cosine`, quantization `int8`
  and dimension 768, and the report names it with `UGS0902` and stage 8

#### Scenario: A search-time parameter in a schema is refused
- **WHEN** a field declares `@index(ann, metric: cosine, ef_search: 64)`
- **THEN** the schema is refused with `UGS0524` naming `ef_search`

#### Scenario: A function is recorded for stage 7
- **WHEN** Gen5's `fn by_id(node: NodeRef) -> Node? = Node(id = node)` is compiled
- **THEN** its body is type-checked, the catalog version holds the function, and the report names it with `UGS0902`
  and stage 7

#### Scenario: Gen5's unenforced declarations are counted once each
- **WHEN** Gen5 is compiled
- **THEN** the report holds twenty `UGS0902` records: one for `Embedding`'s index, fourteen for its functions and five
  for its virtual derivations

### Requirement: The corpus decides conformance
The conformance corpus SHALL hold, written independently of the compiler: for every error code of `SCHEMA.md` section
10.4 from `UGS0101` on, a minimal source refused with exactly that error, with its expected location and fix, and for
every warning and note code a minimal valid source whose expectation lists it; valid
sources, among them the aviation example, `deepbrain-gen5.ugs`, modules of the corpus's own, and one per construct of section 14's
stage 6.6 rows; and for each valid source the exact warnings and notes expected. The compiler SHALL raise exactly the
expected diagnostics for every source of the corpus.

#### Scenario: Every invalid source raises exactly its diagnostic
- **WHEN** each invalid source of the corpus is compiled
- **THEN** each is refused with exactly the diagnostic its expectation names, at the expected location

#### Scenario: Every valid source compiles with exactly its notes
- **WHEN** each valid source of the corpus is compiled
- **THEN** each compiles, and its report holds exactly the warnings and notes its expectation lists

#### Scenario: Gen5 compiles with no error
- **WHEN** `specification/schema/deepbrain-gen5.ugs` is compiled into an empty tenant
- **THEN** it compiles with no diagnostic of kind error, and its report equals the corpus's expectation for it
