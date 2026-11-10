## ADDED Requirements

### Requirement: Type prefixes are allocated in declaration order and never reissued
The metadata group SHALL allocate each declared statement, entity and relation type a 16-bit type prefix from 3, after
0 (invalid), 1 (`Resource`) and 2 (`Statement`): the first catalog version holding a type assigns the next free
prefix to each new type in the order of the types' declarations in the installed sources, in an order the sources alone decide, schemas by name as UTF-8 bytes, then modules, each after what it extends
and ties by name, each source in text order, and a later version assigns the next free prefixes to the types it adds. An abstract type SHALL hold a prefix. A prefix SHALL never be reassigned, renumbered or reused. A version that
would pass 65,532 declared types in its tenant, the prefixes 3 to 65,534, SHALL be refused with `UGS2005`; prefix
65,535 SHALL never be allocated, so the next free prefix and a prefix range's end fit 16 bits.

#### Scenario: The aviation prefixes
- **WHEN** the aviation schema of `SCHEMA.md` section 16 is installed into an empty tenant
- **THEN** `Asset`, `Aircraft`, `Engine`, `Avionics`, `Organization`, `Company`, `Authority`, `Owns`, `PartOf`,
  `Agreement`, `Supply`, `Lease` and `Certifies` hold the prefixes 3 to 15 in that order

#### Scenario: Gen5's first prefixes follow its text
- **WHEN** `deepbrain-gen5.ugs` is installed into an empty tenant
- **THEN** `Statement` holds prefix 3, `Reading` 4, `Asserted` 5, `Node` 6 and `Relation` 7, and its 165 types hold
  the prefixes 3 to 167

#### Scenario: Schemas are numbered before the modules extending them
- **WHEN** one Install holds a module declaring `entity Glider : Aircraft { }`, listed first, and the aviation schema
- **THEN** the aviation types hold the prefixes 3 to 15, and `Glider` holds 16

#### Scenario: Two schemas are numbered by name
- **WHEN** one Install holds the schemas `zoo`, listed first, and `aviation`
- **THEN** the aviation types hold the prefixes 3 to 15, and `zoo`'s types follow from 16

#### Scenario: The prefixes of a tenant run out
- **WHEN** an Install would give a tenant its 65,533rd declared type, which prefix 65,535 would hold
- **THEN** it is refused with `UGS2005`, and the tenant's catalog is unchanged

### Requirement: A type's prefix set is sorted ranges
The catalog SHALL hold, per type and per version, its prefix set, its own prefix and its descendants', as sorted
half-open ranges with adjacent ranges merged. A type test SHALL be a check against those ranges, and a scan of a
supertype one range scan per range.

#### Scenario: A tree declared contiguously is one range
- **WHEN** the aviation schema is installed
- **THEN** `Asset`'s prefix set is the one range 3 to 6 inclusive, `Organization`'s 7 to 9 and `Agreement`'s 12 to 14

#### Scenario: A subtype added by a module adds a range
- **WHEN** a module installed with the aviation schema declares `entity Glider : Aircraft { }`, which takes prefix 16
- **THEN** `Asset`'s prefix set is the two ranges 3 to 6 and 16, `Aircraft`'s the two ranges 4 and 16, and a type
  test of a `Glider` against `Asset` holds

### Requirement: Keys have one canonical encoding
The canonical encoding of a key SHALL be, over the key's attributes in the order `@key` lists them, the concatenation
of each component's 4-byte big-endian length and its bytes, the bytes being the component's canonical lexical form in
UTF-8: the text itself for `String` and `Iri`; the form `PROTOCOL.md` section 10.5 gives for `Bool`, the
integer types, `Decimal`, `Date` and `DateTime`, a `DateTime` normalised to UTC first; the member's name for an enum;
the declared name for a `Unit`. A key attribute SHALL be required and of a keyable type (`UGS0301`, `UGS0306`), named
by a `@key` on its own type (`UGS0307`), and a subtype of a keyed type SHALL declare no key (`UGS0302`).

#### Scenario: A single string key
- **WHEN** the key `EN-4471-B` of an `Asset` is encoded
- **THEN** the bytes are `00 00 00 09` followed by the 9 UTF-8 bytes of `EN-4471-B`

#### Scenario: A composite key concatenates its components
- **WHEN** a type declares `@key(code, year)` with `code: String` and `year: Int32`, and the key `("AB", 2026)` is
  encoded
- **THEN** the bytes are `00 00 00 02 41 42 00 00 00 04 32 30 32 36`

#### Scenario: A DateTime component is normalised to UTC
- **WHEN** the keys `2026-11-01T12:00:00+02:00` and `2026-11-01T10:00:00Z` of one `DateTime` key attribute are encoded
- **THEN** the two encodings are equal

#### Scenario: An optional key attribute is refused
- **WHEN** a type declares `@key(id)` and `id: String Optional`
- **THEN** the schema is refused with `UGS0301`

### Requirement: Buckets and claims come from the key space
A key's key space SHALL be the prefix and the name of the type that declares the key. A keyed instance's bucket SHALL
take its low 16 bits from the top 16 bits of XXH3-64, seed 0, of the key space's prefix as 2 big-endian bytes
followed by the canonical key, and its top 8 bits from the tenant, so every instance of a key's type tree with one
key hashes to one bucket whatever its concrete type. A `@key`'s uniqueness claim SHALL be kept in that bucket, hashed
from the same input. A `@unique` value's claim SHALL be at `Bucket::of_value(tenant, predicate, value)`. A keyless
entity SHALL take the bucket its handle states, and an unkeyed relation instance its anchor participant's bucket.

#### Scenario: A subtype's instance hashes with its key's declaring type
- **WHEN** an `Engine` and an `Aircraft` are each given the key `EN-4471-B` in one tenant
- **THEN** both compute the bucket of prefix 3, `Asset`'s, and that key, so one partition decides which of them is
  created

#### Scenario: A keyed instance's bucket matches the XXH3 reference
- **WHEN** the bucket of the `Asset` key `EN-4471-B` in tenant 0 is computed
- **THEN** its low 16 bits equal the top 16 bits of the xxHash reference implementation's XXH3-64, seed 0, of `00 03
  00 00 00 09` followed by the 9 bytes of `EN-4471-B`

#### Scenario: An unkeyed relation instance lives at its anchor
- **WHEN** an `Owns` instance is inserted for an asset in bucket `b`
- **THEN** the instance's ID carries bucket `b`

### Requirement: Keyed instances are named by an IRI minted from the key
A keyed entity's and a keyed relation instance's IRI SHALL be `{base}{KeySpace}/{key}`: the instance base of its
graph, the key space's name, `/`, and the key's components joined by `/`, each percent-encoded, in uppercase hexadecimal digits, so that every byte
outside RFC 3986's unreserved set is encoded. A type with `@iri(template)` SHALL mint the template with each `{attr}`
placeholder replaced by its percent-encoded component. The compiler SHALL refuse a template from which the key cannot
be read back (`UGS0304`) and two templates one of which is a prefix of the other (`UGS0305`). A write creating an instance under a subject IRI its type
does not mint for it, a keyed type's instance under another IRI than its key mints or under a blank node, or a keyless type's
instance under an IRI in place of a blank node, SHALL be refused at commit with `UGS1038` (IriNotMinted). An IRI of a
handle's form that the dictionary does not hold SHALL be refused earlier, at the write naming it, by the term's rule
(term-dictionary, "Handle IRIs are held both ways"). Two schemas or two graphs with one base SHALL be refused
(`UGS0309`).

#### Scenario: An aviation key mints its IRI
- **WHEN** an `Engine` with `serial` `EN-4471-B` is created in the graph `production`
- **THEN** its IRI is `https://example.org/aviation#Asset/EN-4471-B`

#### Scenario: A Gen5 identity is percent-encoded
- **WHEN** a `Company` with `id` `ORG:acme` is created in the graph `ontology`
- **THEN** its IRI is `https://example.org/deep_brain#Node/ORG%3Aacme`

#### Scenario: A template without a separator between placeholders is refused
- **WHEN** a type with `@key(a, b)` declares `@iri("https://ex.org/x/{a}{b}")`
- **THEN** the schema is refused with `UGS0304`

#### Scenario: A subject IRI that the key does not mint is refused
- **WHEN** quads create an `Engine` under the subject `https://example.org/aviation#Asset/OTHER` with `serial`
  `EN-4471-B`
- **THEN** the commit is refused with `UGS1038`, naming the subject and the IRI the key mints

#### Scenario: A keyless instance is created under a blank node
- **WHEN** quads create a Gen5 `Reading`, with its `valid`, a `value` and its `GeneratedBy` link, under the subject
  `<https://example.org/deep_brain#Reading/mine>`
- **THEN** the commit is refused with `UGS1038`, and the same quads under a blank node commit

#### Scenario: A keyed instance is not created under a blank node
- **WHEN** quads create a `Company` with `id` `ORG:acme` under a blank node
- **THEN** the commit is refused with `UGS1038`, naming the IRI the key mints

### Requirement: Keyless instances are named by a handle
A keyless entity and a relation instance without `@key` that is named by an IRI SHALL take the IRI
`{base}{Type}/{handle}`: the instance base of its graph, its concrete type's name, `/`, and a handle of its bucket in
6 and 104 random bits in 26 lowercase hexadecimal digits, joined by `-`. The creating partition SHALL draw the bucket
from those it owns in the tenant's run, or take the anchor's for a relation instance, and the bits uniformly from its
injected random source, drawing again on a collision. The dictionary SHALL hold the IRI and the ID both ways, the view
SHALL present it as an IRI, and a later request SHALL name the instance by it. A handle SHALL show its bucket and
nothing else of the ID.

#### Scenario: A keyless entity's handle
- **WHEN** partition 1 creates a Gen5 `Reading` with its random source seeded to a recorded value
- **THEN** its IRI is `https://example.org/deep_brain#Reading/` followed by 6 hexadecimal digits naming a bucket
  partition 1 owns, `-` and 26 hexadecimal digits, and the same seed mints the same IRI

#### Scenario: A relation instance's handle carries its anchor's bucket
- **WHEN** a `Supply` instance is inserted whose supplier lives in bucket `b`
- **THEN** its IRI is `https://example.org/aviation#Supply/`, `b` in 6 lowercase hexadecimal digits, `-` and 26
  hexadecimal digits

#### Scenario: A handle names its instance in a later request
- **WHEN** a request names a `Supply` instance by the handle IRI a result gave
- **THEN** the dictionary resolves it to the instance's ID at the partition owning the bucket the handle states

### Requirement: An IRI routes without a lookup
Any node SHALL find an instance's bucket from its IRI and the catalog: for a keyed IRI, the type segment names a key
space, the key reads back from the remaining segments or from the template's literal text, and the key's hash gives
the bucket; for a handle IRI, its first six digits are the bucket. System IDs SHALL never appear in an IRI.

#### Scenario: The frontend routes a keyed IRI
- **WHEN** the frontend binds a transaction whose first subject is `https://example.org/deep_brain#Node/ORG%3Aacme`
- **THEN** it reads the key space `Node`, decodes the key `ORG:acme`, and binds to the partition owning the bucket
  computed from `Node`'s prefix and that key, where a `Company` with that `id` was created

### Requirement: Vocabulary terms have fixed IRIs
The compiler SHALL mint the vocabulary IRIs of `SCHEMA.md` section 7.4 under the namespace's base: `{base}{Type}` for
a type, `{base}{attribute}` or its `@iri` for an attribute, `{base}{nav}` for a navigation name,
`{base}{Relation}_{role}` for a role of a node-encoded relation, `{Relation}` the type declaring the role,
`{base}{Enum}` for an enum and `{base}{Enum}_{member}` for a member, `{base}{name}` for a derived name,
`{base}{graph}` for a graph, and the four interval
bound predicates `{base}{attribute}_start`, `_end`, `_start_precision` and `_end_precision`. The built-in fields SHALL
be `ugdb:asserted_start`, `ugdb:asserted_end` and `ugdb:live` in every schema.

#### Scenario: The aviation vocabulary
- **WHEN** the aviation schema is compiled
- **THEN** its predicates include `av:serial`, `av:tags`, `av:owner`, `av:certification`, `av:Supply_supplier`,
  `av:Supply_buyer`, `av:Supply_item`, `av:term_start` and `av:term_end`, each under
  `https://example.org/aviation#`
