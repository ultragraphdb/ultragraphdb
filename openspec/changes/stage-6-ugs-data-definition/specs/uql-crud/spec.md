## ADDED Requirements

### Requirement: A Query of language 3 runs the UQL CRUD subset
The frontend SHALL accept a Query (`PROTOCOL.md` section 7.4) of `language` 3 in a governed tenant and compile its
text against the catalog version in effect for the transaction's tenant. The subset SHALL be the requests of
`specification/query/QUERY-ULTRA-QL.md` that this grammar derives, each with UQL's meaning:

```
request   = [ "use" qident ] ( query | mutation ) ;
query     = selector [ "limit" int ] shape ;
selector  = qident [ "(" [ fitem { "," fitem } ] ")" ] ;
fitem     = ident cmp_op value | ident ":" selector | "is" qident | "with" "retracted" ;
value     = literal | "$" ident | "[" value { "," value } "]" ;
shape     = "{" sfield { sfield } "}" ;
sfield    = [ ident ":" ] ident | ".." ;
mutation  = "insert" qident values | "update" selector values | "delete" selector | "retract" selector ;
values    = "{" [ assign { "," assign } ] "}" ;
assign    = ident ":" ( value | selector | "[" selector { "," selector } "]" ) ;
```

Tokens and literals SHALL be UGS's. A request of UQL outside the subset SHALL be refused with `UQL0901`, the message
naming the construct and the stage that executes it; a token the lexer does not know with `UQL0002`, a malformed
literal with `UQL0003`, and a token the grammar does not expect with `UQL0004`. A refusal SHALL travel as code 29
(QueryRefused), `detail` the diagnostic's number with 3 in bits 16 to 23, the message beginning with the code and its
`line:column`, and SHALL leave the transaction open. A Query of `language` 3 in a tenant with no schema SHALL be
refused with `UQL0104`.

#### Scenario: A construct outside the subset names its stage
- **WHEN** a client sends `Company(id = "ORG:acme") { id memberships { iri } }`
- **THEN** it is refused with code 29, `detail` 901 with language 3, the message beginning `UQL0901` and naming the
  nested shape, and the transaction stays open

#### Scenario: An upsert waits
- **WHEN** a client sends `upsert Company(id = "ORG:acme") { }`
- **THEN** it is refused with `UQL0901` naming `upsert`

#### Scenario: A syntax error is located
- **WHEN** a client sends `insert Company { id "ORG:acme" }`
- **THEN** it is refused with `UQL0004` at line 1, column 21, the message naming `:` as expected

### Requirement: A request is typed against the catalog before it runs
Every name of a request SHALL resolve in the schema of its working graph, the graph `use` names or the tenant's
`@default` graph (`UQL0104` for none or an unknown one), bare or qualified by an imported schema's name. A name the
catalog does not declare SHALL be refused with `UQL0101`, the message listing the nearest declared names; a field
declared only on a subtype of the selector's type with `UQL0103`; a comparison or an assignment of a value to a type
it does not fit with `UQL0201`; and a parameter whose uses disagree on its type, or whose type no use fixes, with
`UQL0202`. A literal SHALL take the type of the field it is compared with or assigned to. A parameter the text uses
and the request does not give, or gives and the text does not use, SHALL be refused with code 11 (Parameters).

#### Scenario: An unknown attribute lists the nearest names
- **WHEN** a client sends `Company(id = "ORG:acme") { lable }` under Gen5
- **THEN** it is refused with `UQL0101`, the message naming `label`

#### Scenario: A value of the wrong type is refused before it runs
- **WHEN** a client sends `insert Company { id: "ORG:acme", inferred: "yes" }`
- **THEN** it is refused with `UQL0201` naming `inferred` and `Bool`, and nothing is written

#### Scenario: A subtype's field needs its subtype
- **WHEN** a client sends `Asset(serial = "EN-4471-B") { thrust_kn }` under the aviation schema
- **THEN** it is refused with `UQL0103` naming `Engine`

#### Scenario: A parameter takes its field's type
- **WHEN** a client sends `Company(id = $id) { label }` with `$id` bound to the string `ORG:acme`
- **THEN** the query runs with `$id` typed `String`

### Requirement: Insert creates one instance
`insert T { … }` SHALL create one instance of the concrete type `T` in the working graph, with each assigned attribute
and each role assigned by a point selector: a selector fixing its type's whole key, or `iri`, by equality, which
compiles to the participant's IRI with no read. An abstract `T` SHALL be refused with `UQL0501`, an assignment to a
derived name with `UQL0502`, and a role selector that is no point selector with `UQL0507`. The frontend SHALL send the
instance's triples of the RDF view in the request's transaction, a keyed instance under the IRI its key mints and a
keyless one under a blank node, so the partition validates it as it validates quads; every commit rule SHALL be the
commit's, refused with its `UGS1xxx` number. An insert SHALL answer one Result of no columns and no rows, flagged
`LAST`.

#### Scenario: An entity is inserted and its defaults applied
- **WHEN** a client sends `insert Company { id: "ORG:acme", label: "Acme" }` under Gen5 and commits
- **THEN** the commit succeeds, and the instance `https://example.org/deep_brain#Node/ORG%3Aacme` holds `id`,
  `label`, `inferred` `false` and `rank` `normal`

#### Scenario: A relation is inserted between keyed participants
- **WHEN** a `Person` `PER:ada` and a `Company` `ORG:acme` exist, and a client sends `insert MemberOf { member:
  Person(id = "PER:ada"), collection: Company(id = "ORG:acme") }` and commits
- **THEN** the commit succeeds with one `MemberOf` instance anchored at the company, whose role triples name both
  participants

#### Scenario: A set and an enum are assigned by value
- **WHEN** a client sends `insert Aircraft { serial: "AC-000001", commissioned: 2020-01-01, status: active, tags:
  ["a320", "leased"], tail_number: "D-AIAB" }` under the aviation schema and commits
- **THEN** the commit succeeds, and the aircraft holds two `av:tags` triples and `av:status` `"active"`

#### Scenario: An abstract type is refused before it runs
- **WHEN** a client sends `insert Agent { id: "AGT:x" }`
- **THEN** it is refused with `UQL0501`

#### Scenario: A role needs a point selector
- **WHEN** a client sends `insert MemberOf { member: Person(rank = normal), collection: Company(id = "ORG:acme") }`
- **THEN** it is refused with `UQL0507` naming `member`

#### Scenario: A commit rule refuses at commit with its number
- **WHEN** a client sends `insert Company { id: "acme" }` and commits
- **THEN** the Query is answered, and the Commit is refused with code 7, `detail` 1010

#### Scenario: Statements of one transaction commit together
- **WHEN** a client begins a transaction, sends `insert Post { id: "POST:cto" }` and `insert PostIn { post: Post(id =
  "POST:cto"), organisation: Company(id = "ORG:acme") }`, and commits
- **THEN** the commit succeeds; and the first statement committed alone is refused with `UGS1014`

### Requirement: A root query reads instances by type and filter
`T(filters) { shape }` SHALL answer the live instances of `T` and its subtypes in the working graph that satisfy every
filter item, at the transaction's read point: an item comparing an own attribute or `iri` with a value by `=`, `<>`,
`<`, `<=`, `>` or `>=`, `is S` narrowing to the subtype `S`, a role binding on a relation type, and `with retracted`
admitting retracted instances. A selector fixing its type's whole key or `iri` by equality SHALL be a point lookup
routed to the instance's home with no scan; a relation selector binding its anchor role SHALL be a forward read, one binding another role a reverse read
as `PROTOCOL.md` section 11.11 serves one, and one binding no role a scan of its type. The shape SHALL hold attributes of the selector's type, the built-in fields `type`, `iri`,
`asserted` and `live`, an alias before any of them, and `..` for every attribute the type declares. The reply SHALL be
Result batches of one row per instance and one column per shape field, named by its alias or its name: a scalar or an
enum member as its term, `type` as the concrete type's name, `iri` as an IRI, or `ABSENT` for an edge-encoded instance that holds no reifier, a record, interval, list or set as one
`rdf:JSON` literal in `SCHEMA.md` section 11.2's canonical form, and an absent optional value as `ABSENT`. `limit n`
SHALL end the rows after `n`. No row SHALL hold a system ID.

#### Scenario: A point lookup by key
- **WHEN** the `Company` `ORG:acme` with `label` `Acme` exists and a client sends `Company(id = "ORG:acme") { id label
  rank inferred live iri type }`
- **THEN** the reply is one row: `"ORG:acme"`, `"Acme"`, `"normal"`, `false`, `true`,
  `<https://example.org/deep_brain#Node/ORG%3Aacme>` and `"Company"`, and no partition scanned a type's range

#### Scenario: A supertype selects its subtypes
- **WHEN** a `Company` and a `University` exist and a client sends `Organization { id type }`
- **THEN** the reply holds both, each with its concrete type's name

#### Scenario: An absent optional value is absent
- **WHEN** a `Company` without `description` is read with `{ id description }`
- **THEN** its row's `description` cell is `ABSENT`

#### Scenario: A filter on an attribute
- **WHEN** three `Claim`s hold `confidence` 0.2, 0.6 and 0.9 and a client sends `Claim(confidence >= 0.5) { id }`
- **THEN** the reply holds the two claims with 0.6 and 0.9

#### Scenario: A relation is found by its anchor
- **WHEN** a `MemberOf` exists for `PER:ada` in `ORG:acme` and a client sends `MemberOf(collection: Company(id =
  "ORG:acme")) { iri live }`
- **THEN** the reply is one row with the instance's handle IRI and `true`

#### Scenario: A relation is found by a role that is not its anchor
- **WHEN** a `MemberOf` exists for `PER:ada` in `ORG:acme` and a client sends, in a read-only transaction,
  `MemberOf(member: Person(id = "PER:ada")) { iri }`
- **THEN** the reply is one row with the instance's handle IRI, read through the person's referrer set

#### Scenario: An edge without a reifier has no iri
- **WHEN** an aviation `PartOf` exists and a client sends `PartOf(part: Asset(serial = "EN-4471-B")) { iri type }`
- **THEN** the reply is one row whose `iri` cell is `ABSENT` and whose `type` is `"PartOf"`

#### Scenario: An interval is read as its record
- **WHEN** a `Company` is read with `{ asserted }`
- **THEN** the cell is an `rdf:JSON` literal holding `start` and no `end`

#### Scenario: Every attribute
- **WHEN** an aviation `Engine` is read with `{ .. }`
- **THEN** the columns are `serial`, `commissioned`, `status`, `tags`, `notes`, `embedding` and `thrust_kn`

### Requirement: Update, delete and retract act on what a selector selects
`update S { … }` SHALL replace each assigned attribute's value on every instance `S` selects; on a `@bitemporal` type
each update SHALL be a revision; an `@append_only` type SHALL be refused with `UQL0503` and a derived name with
`UQL0502`. `delete S` SHALL delete every instance `S` selects, under `@on_delete`, as Operation kind 5, or as the delete of its
quad for an edge-encoded instance that holds no reifier; on a
`@bitemporal` type it SHALL be refused with `UQL0504`, the fix naming `retract`. `retract S` SHALL retract every
instance `S` selects, under `@on_retract`, as Operation kind 6; on a type that is not `@bitemporal` it SHALL be
refused with `UQL0505`. The selection SHALL be read inside the request's read-write transaction and recorded in its
read set. A mutation in a read-only transaction SHALL be refused with `UQL0603`. Each SHALL answer one Result of no
columns and no rows, flagged `LAST`.

#### Scenario: An update is read back as a revision
- **WHEN** a client sends `update Company(id = "ORG:acme") { label: "Acme Corp" }`, commits, and reads `Company(id =
  "ORG:acme") { label iri }`
- **THEN** the row holds `"Acme Corp"` and the IRI the instance had before

#### Scenario: A retraction leaves the default read
- **WHEN** a client sends `retract Company(id = "ORG:acme")`, commits, and reads `Company(id = "ORG:acme") { id }` and
  `Company(id = "ORG:acme", with retracted) { id live }`
- **THEN** the first reply holds no row, and the second one row with `live` `false`

#### Scenario: Delete of a bitemporal type names retract
- **WHEN** a client sends `delete Company(id = "ORG:acme")` under Gen5
- **THEN** it is refused with `UQL0504`, the fix `retract Company(id = "ORG:acme")`

#### Scenario: A mutable entity is deleted
- **WHEN** a client sends `delete Aircraft(serial = "AC-000001")` under the aviation schema and commits
- **THEN** the commit succeeds, a read of the aircraft holds no row, and its home holds its tombstone

#### Scenario: Retract of a mutable type is refused
- **WHEN** a client sends `retract Aircraft(serial = "AC-000001")`
- **THEN** it is refused with `UQL0505`

#### Scenario: Update of an append-only type is refused
- **WHEN** a client sends `update Reading(text = "ok") { text: "fine" }` under Gen5
- **THEN** it is refused with `UQL0503`

#### Scenario: A restricted retraction refuses the commit
- **WHEN** the `Company` `ORG:acme` has a live `MemberOf` with no `valid` end, and a client sends `retract Company(id =
  "ORG:acme")` and commits
- **THEN** the Commit is refused with code 7, `detail` 1020

#### Scenario: An update by filter revises each selected instance
- **WHEN** three `Company` instances hold `rank` `normal` and a client sends `update Company(rank = normal) { rank:
  deprecated }` and commits
- **THEN** all three read back with `rank` `deprecated` and `closed` `true`

#### Scenario: A write in a read-only transaction is refused
- **WHEN** a client sends `insert Company { id: "ORG:x" }` in a transaction begun read-only
- **THEN** it is refused with `UQL0603`

### Requirement: A UQL write and its quads are one write
An instance set written through the UQL subset and the same set written as the RDF view's quads SHALL leave equal
datasets, up to the naming of handles and reifiers, and a write the commit refuses through one SHALL be refused
with the same `UGS1xxx` number through the other.

#### Scenario: The corpus's instance sets through both surfaces
- **WHEN** each instance set of the corpus that the subset can express is written through UQL into one tenant and as
  quads into another holding the same schema
- **THEN** the two tenants' RDF views are equal up to the naming of handles and reifiers

#### Scenario: A rule refuses alike through both surfaces
- **WHEN** each rule-breaking instance set of the corpus that the subset can express is written through UQL and as
  quads
- **THEN** both commits are refused with the `UGS1xxx` number its expectation names
