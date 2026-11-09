## ADDED Requirements

### Requirement: A catalog version holds the compiled schema
A tenant's catalog version SHALL hold the entries `SCHEMA.md` section 10.2 lists for everything its sources declare:
Schema, Module, Type, Prefix ranges, Attribute, Predicate, Role, Navigation, Enum, Record, named attribute type,
Quantity, Unit, Derivation, Function, Graph, Classification, Report and Sources, the kinds 1 to 19 of `PROTOCOL.md`
section 8.3. A Type entry SHALL carry the type's
prefix, parent, mutability class, key attributes, IRI template, relation encoding, anchor role and every declared
constraint; a Predicate entry what `PROTOCOL.md` section 4.6's kind-8 record carries, one per attribute name per base
or `@iri`, per role of a node-encoded relation, per anchor navigation name of an edge-encoded relation and per
interval bound; the Sources entry each installed source's text by name.

#### Scenario: The aviation catalog version, entry by entry
- **WHEN** the aviation schema of `SCHEMA.md` section 16 is compiled into an empty tenant
- **THEN** the catalog version equals, entry by entry, the expectation the corpus holds for it, written by hand from
  `SCHEMA.md`: 13 Type entries with the prefixes 3 to 15, `Owns`, `PartOf` and `Certifies` edge-encoded and
  `Agreement`, `Supply` and `Lease` node-encoded, and the predicates section 16 lists

#### Scenario: A shared attribute is one predicate and one attribute entry per declaring type
- **WHEN** Gen5 is compiled
- **THEN** the catalog version holds one Predicate entry `db:uri` and three Attribute entries for it, declared by
  `Object`, `Event` and `Information`

### Requirement: A catalog version has one encoding
A catalog version SHALL have one canonical encoding, specified in `specification/schema/CATALOG.md` by the six rules of
`SCHEMA.md` section 10.2: a body in `PROTOCOL.md` section 4.1's layout whose entry table is Describe's entry record;
each entry self-contained, a body of its own with its own strings; entries ordered by kind, then by name as UTF-8
bytes, every field present. Its fixed part SHALL hold a format version and an XXH3-64 of the bytes after it. Its
decoder SHALL check the checksum, then every rule of the encoding, and SHALL refuse every byte sequence that is not
the canonical encoding of some version, naming the offset, without a panic. Encoding a decoded version SHALL give the
bytes decoded. A version SHALL hold an index from type prefix to Type entry, and a role SHALL hold a version decoded
as views over its bytes, built once. An instance check, a `@limit` condition and a computed derivation SHALL be
stored as typed postfix programs of fixed-width instructions, whose stack discipline, operand bounds and depth, at
most 64, the decoder SHALL verify, and SHALL be evaluated without recursion and without allocation. A function's body
and a path derivation's SHALL be stored as their text. A
role that cannot decode a version its log or its metadata group names SHALL stop. The metadata group SHALL treat the
bytes as opaque.

#### Scenario: Encoding round-trips
- **WHEN** the compiled Gen5 catalog version is encoded, decoded and encoded again
- **THEN** the two encodings are byte for byte equal

#### Scenario: Entries out of order are refused
- **WHEN** the bytes of a catalog version are altered so that two Type entries swap places
- **THEN** the decoder refuses them, naming the offset of the second entry

#### Scenario: The hand-encoded example decodes
- **WHEN** the bytes `CATALOG.md` works by hand for its schema of one entity with one attribute are decoded
- **THEN** the decoder answers that schema's entries, and encoding them gives the same bytes

#### Scenario: A flipped byte fails the checksum
- **WHEN** one byte of the compiled aviation version is changed and the version is decoded
- **THEN** the decoder refuses it naming the checksum, and a partition loading it stops

#### Scenario: A program that underflows its stack is refused
- **WHEN** a version's bytes are altered so that an instance check's program pops a value it never pushed, and the
  checksum is recomputed
- **THEN** the decoder refuses the version naming the entry and the instruction, and no check of it is evaluated

#### Scenario: A type is reached by its prefix
- **WHEN** the compiled Gen5 version is decoded and the type of prefix 6 is asked for
- **THEN** the views answer `Node` with its fields and its key, having compared no name

#### Scenario: Declaration order does not change the encoding of what is declared alike
- **WHEN** two sources differ only in the order of two enum declarations
- **THEN** their catalog versions' Enum entries are byte for byte equal

### Requirement: The schema id identifies what a principal reads of a version
The schema id SHALL be `PROTOCOL.md` section 11.7's hash of the Describe body, covering every entry the principal may
read. It SHALL be equal for one catalog version on every node, across a restart of every role, and SHALL differ after
an Install changes the version. Recording a default entry SHALL keep it.

#### Scenario: The schema id survives a restart
- **WHEN** Gen5 is installed, Describe is answered, every role is restarted, and Describe is answered again
- **THEN** the two schema ids are equal

#### Scenario: The schema id's golden value
- **WHEN** the schema id of the wire specification's golden Describe body is computed
- **THEN** it is `0x5000B62149F4DB68`

### Requirement: Install compiles a tenant's sources into a new version
An Install SHALL name a tenant, the catalog version it builds on and one or more sources, each a name and a text. The
metadata group's owner SHALL compile the sources the tenant holds together with those sent, through every phase of
`SCHEMA.md` section 10.1, allocate prefixes for the new types, and commit the catalog version as pending. The reply
SHALL carry the version's number, the epoch it takes effect at, the report and the warnings. A diagnostic of kind
error SHALL refuse the Install whole with code 28 (SchemaRefused), `detail` the count of diagnostics of kind error, the message
holding each diagnostic with its code and location, and the tenant's catalog SHALL be unchanged. An Install SHALL take
the action `declare` on the tenant at the frontend's gate. After the checks of `version` and of the tenant's schema, and before it checks that
the tenant holds no statement, write-path declaration or default-entry record, the frontend SHALL fence the tenant:
every Write, Operation, Commit, DirectWrite, ConditionalWrite and Drop naming the tenant SHALL be refused with code 31
(Fenced), `detail` 0, the message naming the tenant, writing nothing and ending a transaction, reads going on, from
then until the version takes effect or the Install is refused, the fence surviving a
restart of the frontend, and the check SHALL wait for the tenant's commits in flight and then read the tenant's partitions for a statement.

#### Scenario: Gen5 installs into a fresh tenant
- **WHEN** a client installs `deepbrain-gen5.ugs` into a tenant with no catalog version, naming version 0
- **THEN** the reply names version 1 and its effective epoch, its report equals the corpus's expectation for Gen5, and
  its warnings hold the `UGS0902` of the `Embedding` index

#### Scenario: A source with an error installs nothing
- **WHEN** a client installs two sources, the aviation schema and a schema with one field typed `Strng`
- **THEN** the Install is refused with code 28, `detail` 1, the message holding `UGS0201` with its file, line and
  column, and the tenant holds no catalog version

#### Scenario: A first schema fences its tenant until it takes effect
- **WHEN** Gen5's Install is answered with effective epoch `e`, and a client writes a `Company` before `e` and again at
  `e`
- **THEN** the first write is refused with code 31, the message naming the tenant, and writes nothing, and the second
  commits, validated against Gen5

#### Scenario: A commit in flight is waited for
- **WHEN** a transaction's Commit writing `<a> <p> "x"` is in flight in an empty tenant when an Install arrives
- **THEN** the Install's check runs after the Commit is answered, and the Install is refused with code 7 naming Drop
  of kind 4, since the tenant then holds a statement, whether or not its default-entry record has been recorded

### Requirement: A tenant takes one Install until stage 9.5
An Install SHALL be accepted into a tenant that holds no schema, with one or more sources compiled together: schemas,
and modules extending them, in any order in the request. An Install into a tenant that holds a schema SHALL be
refused with code 7 (Refused), `detail` 0, the message naming Install and stage 9.5, and an Install holding a
migration source likewise. An Install building on a version that is no longer current SHALL be refused with code 2
(Conflict), kind 5 (Catalog), checked before the tenant's schema. An Install into a tenant that holds a statement, or whose catalog holds a
write-path declaration or a default-entry record, SHALL be refused with code 7, `detail` 0, the message naming Drop of kind 4. A
Drop of kind 4, naming the tenant's current catalog version, SHALL set a governed tenant's catalog to the initial one
and remove the tenant's default-entry records, after which the tenant takes an Install again.

#### Scenario: Several sources install together
- **WHEN** a client installs, in one request, a module declaring `entity Glider : Aircraft { }` and then the aviation
  schema it extends
- **THEN** the reply names version 1, and Describe answers both sources and a Type entry for `Glider` whose parent is
  `Aircraft`

#### Scenario: A second Install waits for stage 9.5
- **WHEN** a tenant holds `deep_brain` and a client installs any source, naming the current version
- **THEN** the Install is refused with code 7, the message naming stage 9.5, and the catalog is unchanged

#### Scenario: A stale base version is refused first
- **WHEN** two clients each install a schema into one empty tenant naming catalog version 0, one after the other
- **THEN** the first is answered with version 1, and the second is refused with code 2, kind 5

#### Scenario: A tenant holding ungoverned data takes no schema
- **WHEN** a tenant holds a default-entry record for `<nickname>` and a client installs the aviation schema
- **THEN** the Install is refused with code 7, the message naming Drop of kind 4

#### Scenario: A dropped tenant takes a new schema
- **WHEN** a tenant holds the aviation schema and an `Engine`, is dropped with a Drop of kind 4 naming its catalog
  version, and a client installs Gen5
- **THEN** the Install succeeds, Describe answers the source `deep_brain` alone, and a read of every quad of the tenant
  answers no row

### Requirement: A catalog version is bounded
The frontend SHALL refuse a compiled version whose bytes pass `catalog_bytes_max` with code 7, `detail` the bound,
the message naming the version's size, and install nothing. A version past one frame SHALL travel between roles in
continuation Catalog frames.

#### Scenario: A version past the bound is refused
- **WHEN** `catalog_bytes_max` is set below the size of Gen5's compiled version and Gen5 is installed into an empty
  tenant
- **THEN** the Install is refused with code 7, `detail` the bound, and the tenant holds no catalog version

#### Scenario: A version past one frame reaches a partition in continuation frames
- **WHEN** a schema of the corpus built to compile past one Catalog frame is installed and a storage process loads it
- **THEN** the partition holds the version whole, and its decoded entries equal the metadata group's

### Requirement: Describe answers the sources and the entries
The frontend SHALL answer Describe from the metadata group, asking no other role, with the catalog version in effect
at the latest closed epoch: the installed sources byte for byte, and every catalog entry by its record kind, in the
canonical order, as far as the principal may read them, across continuation frames where the answer passes one frame.
A tenant with no schema SHALL be described as stage 5.5 describes it.

#### Scenario: Describe returns the source byte for byte
- **WHEN** Gen5 is installed and its version is in effect, and a client sends Describe
- **THEN** the source named `deep_brain` in the answer equals the installed file byte for byte

#### Scenario: Describe returns the typed entries
- **WHEN** the aviation schema is installed and in effect, and a client sends Describe
- **THEN** the answer holds 13 Type entries, each with its prefix, parent and encoding, the entries ordered by kind
  and then by name

#### Scenario: A Describe past one frame continues
- **WHEN** a schema of the corpus built so that its Describe answer passes the floor of `max_frame` is installed, and a
  client at that floor sends Describe
- **THEN** the answer arrives in several frames, the last flagged `LAST`, and their sections concatenate to the whole
  answer

### Requirement: A tenant with a schema is governed
A tenant whose catalog version holds at least one schema SHALL be governed: every write SHALL name a declared graph or
the tenant's `@default` graph (`UGS1031`), every subject SHALL be an instance of a declared concrete type or, under an
`@open` schema, a `Resource`, and every predicate SHALL be declared on the subject's type or admitted by `@open`
(`UGS1001`). A write-path declaration, a change record of kind 8, 9 or 10, SHALL be refused in a governed tenant with
code 7, `detail` the record's index, the message naming Install, and a Drop of kind 1 naming a predicate a schema
declares with code 7, `detail` 0, naming stage 9.5. A DirectWrite or a ConditionalWrite in a governed tenant SHALL carry only
changes whose predicates the catalog's request-typing facts mark blind (`SCHEMA.md` section 9.6), refused with code 7
otherwise, `detail` the term's index, the message naming the predicate and a read-write transaction: a rule over the
instance's own fields is validated in the frame's turn, and a claim of a key or a `@unique` value, or a rule reading
beyond the instance, takes a transaction. An Operation of kind 1 or 2 SHALL be refused in a governed tenant with code
7, `detail` 0, the message naming the kind; an Operation of kind 5 or 6 naming an IRI no instance holds, and either
kind in a tenant with no schema, with code 7, `detail` 0. A predicate `@open` admitted SHALL be dropped as any other. A tenant with no schema SHALL keep the ungoverned behaviour of `PROTOCOL.md` section 11.13.

#### Scenario: An undeclared predicate is refused
- **WHEN** Gen5 is in effect and a transaction writes `<…#Node/ORG%3Aacme> db:nickname "Al"` on a `Company`
- **THEN** the commit is refused with code 7, `detail` 1001, naming `db:nickname` and `Company`

#### Scenario: An open type admits undeclared attributes
- **WHEN** a schema declares `entity Note @open @key(id) { id: String }` and a transaction writes an undeclared
  predicate on a `Note`
- **THEN** the commit succeeds, and the metadata group records a default entry for the predicate

#### Scenario: An undeclared graph is refused
- **WHEN** Gen5 is in effect and a transaction writes into the graph `<https://example.org/deep_brain#scratch>`
- **THEN** the commit is refused with code 7, `detail` 1031

#### Scenario: A write-path declaration is refused in a governed tenant
- **WHEN** Gen5 is in effect and a Write carries a kind-8 record declaring a predicate
- **THEN** it is refused with code 7, the message naming Install

#### Scenario: A keyed creation takes a transaction
- **WHEN** Gen5 is in effect and a DirectWrite carries the triples of a new `Company`
- **THEN** it is refused with code 7, the message naming `db:id` and a read-write transaction, and the same triples
  sent as a Write in a read-write transaction commit

#### Scenario: A blind write goes in one frame
- **WHEN** Gen5 is in effect and a DirectWrite sets `db:description` on an existing `Company`
- **THEN** the reply is a Commit, and the company reads back with the description as a revision

#### Scenario: A retraction of an IRI no instance holds is refused
- **WHEN** Gen5 is in effect and an Operation of kind 6 names `…#Node/ORG%3Anone`, which no instance holds
- **THEN** the commit is refused with code 7, `detail` 0, naming the IRI

#### Scenario: An instance operation needs a schema
- **WHEN** a tenant holds no schema and a transaction sends an Operation of kind 5
- **THEN** it is refused with code 7, `detail` 0

#### Scenario: A tenant without a schema stays ungoverned
- **WHEN** a tenant holds no schema and a transaction writes `<a> <p> "x"` with `p` undeclared
- **THEN** the commit succeeds, and `p` holds a default entry

#### Scenario: Two tenants hold different schemas
- **WHEN** the metadata group holds tenants 0 and 3, tenant 0 holds Gen5 and tenant 3 the aviation schema, and each
  transaction creates an aviation `Engine` in its tenant
- **THEN** the commit in tenant 0 is refused with `UGS1002`, the commit in tenant 3 succeeds, and each tenant's Describe
  answers its own source alone

### Requirement: Every role reads the version in effect
A partition, compute and the frontend SHALL each load the catalog version in effect for a tenant and decode it once.
A commit SHALL be validated against the version in effect at its epoch, and a read-write transaction that executed
against another version than the one in effect at its commit SHALL abort with code 2 (Conflict), kind 5 (Catalog). A
prepared statement SHALL go stale when its tenant's schema id changes.

#### Scenario: A transaction spanning an Install aborts
- **WHEN** a read-write transaction reads in a tenant with no schema, the tenant's Install takes effect, and the
  transaction commits a write
- **THEN** the commit is refused with code 2, kind 5, and a retry executes against the installed version

#### Scenario: A prepared pattern goes stale at the Install
- **WHEN** a quad pattern is prepared in a tenant with no schema, Gen5's Install takes effect, and the statement is
  executed
- **THEN** the execution is refused with code 9, and preparing it again succeeds
