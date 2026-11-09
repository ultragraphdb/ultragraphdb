## ADDED Requirements

### Requirement: A relation type is edge-encoded or node-encoded by its shape
The compiler SHALL fix each relation type's encoding from its roles and its key alone, as `SCHEMA.md` section 11.1
states, and record it in the Type entry. A relation with exactly two roles, both required and single-valued, and no
`@key` SHALL be edge-encoded: one quad `anchor {base}{nav} other`, `nav` the navigation name of the anchor role.
Every other relation SHALL be node-encoded: a subject of the relation's prefix, named by its IRI, with one role triple
`instance {base}{Relation}_{role} participant` per participant and its attributes as its own quads. An edge-encoded
instance SHALL be allocated a reifier, a statement ID carrying the relation's prefix, at insert, exactly when it
carries attributes, its relation is `@multi` or `@bitemporal`, its relation tree holds more than one concrete type, or
a role of any relation in the catalog names its type, a supertype of it or a statement type covering it. An unkeyed instance of either encoding SHALL keep every entry in its anchor's partition, and a keyed relation instance's
entries SHALL be in the partition its key hashes to.

#### Scenario: The aviation encodings
- **WHEN** the aviation schema is compiled
- **THEN** `Owns` is edge-encoded with the triple `asset av:owner organization`, `PartOf` with `part av:assembly
  whole`, `Certifies` with `supply av:certification authority`, and `Agreement`, `Supply` and `Lease` are
  node-encoded

#### Scenario: A link without attributes is one quad
- **WHEN** a Gen5 `AssertedBy` is inserted for a statement and an asserter
- **THEN** the partition holds the one quad `statement db:asserted_by asserter`, and no statement record

#### Scenario: A binary relation below Relation is an edge with a reifier
- **WHEN** a Gen5 `IsAbout` is inserted
- **THEN** the partition holds the quad `information db:about topic` and a statement record whose reifier carries
  `IsAbout`'s prefix, takes the bucket of `information`, and holds the instance's attributes

#### Scenario: A relation with an optional role is a node
- **WHEN** a Gen5 `MemberOf` is inserted without a `role`
- **THEN** the partition holds a subject of `MemberOf`'s prefix in the collection's bucket with the role triples
  `db:MemberOf_member` and `db:MemberOf_collection`

#### Scenario: A bitemporal edge always holds a reifier
- **WHEN** a schema declares a `@set` `@bitemporal` binary relation with an optional valid interval, and an instance
  is inserted without it
- **THEN** the instance holds a reifier carrying `ugdb:asserted_start`, and is retracted by that reifier's IRI

#### Scenario: An attribute-free set relation named by no role has no reifier
- **WHEN** an aviation `PartOf` is inserted
- **THEN** the partition holds the one quad `part av:assembly whole`

### Requirement: The RDF view presents a governed tenant's instances
A read of quad patterns in a governed tenant SHALL answer the quads of `SCHEMA-APG.md` definition 17.1 for the
tenant's instances, each in its instance's named graph: `rdf:type` of the concrete type and every supertype and
statement type, answered from the prefix ranges; each attribute as a literal of `SCHEMA.md` section 5.1's datatype, a
`Set` one triple per member, a `List` and a record one `rdf:JSON` literal in RFC 8785's canonical form, an interval
its bound and precision predicates, a measure with its unit on the value one `ugdb:measure` literal, a catalog type
its class IRI; on a `@bitemporal` instance `ugdb:asserted_start`, `ugdb:asserted_end` and `ugdb:live`; a node-encoded
relation's role triples; and an edge-encoded relation's triple, with `reifier rdf:reifies <<( s p o )>>` and the
attributes on the reifier where one is allocated, a `@symmetric` relation's triple in both directions. An absent
optional value SHALL have no quad. A system ID SHALL never appear.

#### Scenario: An entity's type triples come from its prefix
- **WHEN** a `Company` exists and the pattern `<…#Node/ORG%3Aacme> rdf:type ?t` is read
- **THEN** the rows are `db:Company`, `db:Organization`, `db:Agent`, `db:Endurant`, `db:Node`, `db:Asserted` and
  `db:Statement`

#### Scenario: A supertype pattern scans its prefix ranges
- **WHEN** a `Company` and a `Person` exist and the pattern `?s rdf:type db:Agent` is read
- **THEN** both subjects are returned, and no stored `rdf:type` quad was read

#### Scenario: A list of records is one canonical JSON literal
- **WHEN** a Gen5 `Teloid` holds `meta` with the two `KeyValue` records `(owner, ops)` and `(env, prod)`, in that order
- **THEN** the view holds one `db:meta` triple whose object is the `rdf:JSON` literal
  `[{"key":"owner","value":"ops"},{"key":"env","value":"prod"}]`

#### Scenario: Asserted is read as close times
- **WHEN** a `Company` committed in epoch 12 is read
- **THEN** `ugdb:asserted_start` is the `xsd:dateTimeStamp` the sequencer recorded for epoch 12's close, `ugdb:live`
  is `true`, and there is no `ugdb:asserted_end`

#### Scenario: An edge with attributes is read through its reifier
- **WHEN** an aviation `Owns` with `since` `2026-11-01` exists
- **THEN** the view holds `asset av:owner organization`, a reifier with `rdf:reifies` of that triple term, and
  `reifier av:since "2026-11-01"^^xsd:date`

### Requirement: The default read is the live slice
A read in a governed tenant SHALL see the live slice of `SCHEMA-APG.md` section 15.3: every instance present and not
retracted, each with its newest version, and an instance of a relation that is not `@bitemporal` only while every participant of it is live; a read SHALL add
every instance a `@retracts` role of them reaches, as a read `with retracted` holds it. A read at an earlier epoch SHALL see the versions current at that epoch.

#### Scenario: A retracted instance leaves the default read
- **WHEN** a `Company` is retracted and the pattern `?s rdf:type db:Company` is read
- **THEN** the retracted company is not among the rows

#### Scenario: A link leaves with its participant
- **WHEN** a `Claim` with an `AssertedBy` link is retracted and the pattern `?s db:asserted_by ?o` is read
- **THEN** the link is not among the rows

#### Scenario: A correction still names what it retracted
- **WHEN** a `Supersedes` names a retracted `Claim` as `old` and a live one as `new`
- **THEN** the default read holds the quad `new db:supersedes old` and the old claim's quads with `ugdb:live` `false`

### Requirement: Writes through quads assemble into instances
A client SHALL write a governed tenant's instances as the quads of the RDF view. An instance named by its IRI MAY be
written in any number of Write frames within one transaction; a blank node's label names one new instance within one
Write, so an instance created under a blank node SHALL be created whole in that Write, and a transaction creating an
unkeyed, node-encoded relation instance SHALL bind to the partition owning its anchor participant's bucket. The forms: an entity as its `rdf:type` and attribute triples, a node-encoded relation instance as its `rdf:type`
and role triples under a blank node or its IRI, an edge-encoded instance as its triple, with its attributes as the
properties of that statement. A blank node naming a new keyless instance SHALL be given its handle IRI by the creating
partition. An insert or a delete of a derived property, an inverse navigation name, a node-encoded relation's navigation
property or a super role's triple, SHALL be refused with `UGS1029`.

#### Scenario: A node-encoded instance under a blank node is one Write
- **WHEN** a transaction writes, in one Write, `_:m rdf:type db:MemberOf`, `_:m db:MemberOf_member <person>` and `_:m
  db:MemberOf_collection <org>`
- **THEN** the transaction binds to the organisation's partition, the commit succeeds, and a read of the instance's
  quads holds the two role triples under one handle IRI `https://example.org/deep_brain#MemberOf/…` whose first six
  digits are the organisation's bucket, with its types, its defaults `db:inferred` and `db:rank`,
  `ugdb:asserted_start` and `ugdb:live`

#### Scenario: A blank node does not span Writes
- **WHEN** a transaction writes `_:m rdf:type db:MemberOf` in one Write and `_:m db:MemberOf_member <person>` in
  another
- **THEN** the commit is refused, the two labels naming two instances, the first without its required roles
  (`UGS1015`)

#### Scenario: A keyed relation instance is written one triple at a time
- **WHEN** a schema declares a relation with `@key(code)`, and a transaction writes its `rdf:type`, its `code` and
  each role triple under its minted IRI, each in its own Write
- **THEN** the commit succeeds with one instance

#### Scenario: An edge with a property written in one record
- **WHEN** a transaction writes the edge `asset av:owner organization` with the property `av:since
  "2026-11-01"^^xsd:date`, a change record of kind 2
- **THEN** the commit succeeds with one `Owns` instance holding `since`

#### Scenario: An inverse navigation is not written
- **WHEN** a transaction writes `organization av:assets asset`
- **THEN** the commit is refused with `UGS1029`, the message naming `av:owner` as the triple to write

### Requirement: The view round-trips against the oracle
The engine's RDF view, every quad of the dataset, SHALL equal the oracle's `R(G)` for every instance set the Lean
oracle exports (`lean/schema/ApgModel`), written through quads into a tenant holding its schema, up to a bijection
between `R(G)`'s blank nodes and the IRIs the engine minted for them, and SHALL satisfy the properties the model
states for the engine, proposition 17.3's round trip first. The oracle's files SHALL be committed test data; no Bazel
target runs `lake`.

#### Scenario: The aviation instance sets round-trip
- **WHEN** each aviation instance set of the oracle is written as quads and every quad is read back
- **THEN** the quads read equal that set's `R(G)` file up to the naming of handles and reifiers

#### Scenario: The Gen5 instance sets round-trip
- **WHEN** each Gen5 instance set of the oracle is written as quads into a tenant holding Gen5 and every quad is read
  back
- **THEN** the quads read equal that set's `R(G)` file up to the naming of handles and reifiers
