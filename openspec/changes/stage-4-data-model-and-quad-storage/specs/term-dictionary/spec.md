## ADDED Requirements

### Requirement: IRIs and blank nodes get allocated UIDs
Each IRI and each skolemized blank node SHALL map, per namespace, to one 64-bit UID, never reused. The top 16 bits of a
UID SHALL be reserved for an entity-type prefix and SHALL be zero in this stage. The same IRI SHALL always map to the
same UID, and UID 0 SHALL never be assigned.

#### Scenario: An IRI keeps its UID
- **WHEN** `<a>` is written in two separate commits
- **THEN** both name the same UID

#### Scenario: The type bits are zero
- **WHEN** a UID is assigned to a new IRI
- **THEN** its top 16 bits are zero

### Requirement: Concurrent creation of one IRI yields one UID
Two writes that create the same new IRI concurrently SHALL end with one UID for it. The write committing second SHALL
be refused with `Transaction has been aborted. Please retry`, writing nothing, and its retry SHALL find the first
write's UID.

#### Scenario: The second creator retries
- **WHEN** two writes read before either commits, both create `<new>`, and both commit
- **THEN** the first commits, the second fails with `Transaction has been aborted. Please retry`, and `<new>` has one
  UID

### Requirement: Literals are encoded by content
A literal SHALL be encoded as one tag byte and at most 32 bytes, with no allocation and no read:
- an `xsd:boolean`, an `xsd:integer` within 64 bits, an `xsd:decimal`, `xsd:double`, `xsd:float`, or a date, time
  or duration type SHALL be encoded by value only when its lexical form is canonical, that is, equal to the canonical
  form of its value, and by datatype and lexical form otherwise;
- a string, a language-tagged string with or without base direction, and any other typed literal SHALL keep its lexical
  form, and language tag, inline when each is at most 15 bytes, and as a 128-bit BLAKE2b hash naming its dictionary
  entry otherwise.

Equal terms SHALL encode equally, unequal terms unequally, and decoding SHALL return the exact term.

#### Scenario: A canonical integer is encoded by value
- **WHEN** `"42"^^xsd:integer` is encoded
- **THEN** its encoding holds the value 42 and decodes to `"42"^^xsd:integer`

#### Scenario: A non-canonical integer keeps its lexical form
- **WHEN** `"042"^^xsd:integer` and `"42"^^xsd:integer` are encoded
- **THEN** their encodings differ, and the first decodes to `"042"^^xsd:integer`

#### Scenario: A long string is hashed
- **WHEN** a 40-byte string literal is encoded
- **THEN** its encoding names a 128-bit hash, and decoding through the dictionary returns the string

#### Scenario: A short language-tagged string is inline
- **WHEN** `"chat"@fr` is encoded
- **THEN** its encoding holds `chat` and `fr` inline, and decodes to `"chat"@fr`

### Requirement: The dictionary is a keyspace without versions
The dictionary SHALL map term to UID, UID to term and literal hash to string in a keyspace without versions, read by
point lookups. Entries SHALL be written only in a commit batch, so an aborted write leaves none. A literal's entry SHALL
be written blind: equal strings write equal keys and values. Entries SHALL not be removed in this stage.

#### Scenario: An aborted write leaves no entry
- **WHEN** a write creating `<gone>` is refused at commit
- **THEN** the dictionary holds no entry for `<gone>`

#### Scenario: A read at an older timestamp is unaffected by a newer term
- **WHEN** `<late>` is created at 10 and a read at 5 lists every statement of the default graph
- **THEN** the read returns no statement naming `<late>`

### Requirement: A missing dictionary entry is corruption
A stored UID or literal hash with no dictionary entry SHALL be refused on read with an error naming the key that holds
it, as a corrupted value is.

#### Scenario: A dangling hash is reported
- **WHEN** a list names a literal hash whose dictionary entry is missing and the list is read
- **THEN** the read fails with an error naming the list's key, and nothing panics
