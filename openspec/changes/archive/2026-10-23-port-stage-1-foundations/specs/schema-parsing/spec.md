## ADDED Requirements

### Requirement: Schema text parses into schema updates
Schema text SHALL parse into predicate updates (name, value type, list, directives, tokenizers, index options)
and type updates (name, fields), equal to Dgraph's `schema.Parse` output.

#### Scenario: A uid list predicate parses
- **WHEN** `friend: [uid] .` is parsed
- **THEN** the update is predicate `0-friend`, value type 7, list true

#### Scenario: Parsed schemas match Dgraph's updates
- **WHEN** each schema of the golden schema corpus is parsed
- **THEN** the updates equal Dgraph's

### Requirement: Invalid schemas are refused with Dgraph's errors
The parser SHALL refuse what Dgraph refuses, with Dgraph's error text.

#### Scenario: A duplicate tokenizer is refused
- **WHEN** `name: string @index(term, term) .` is parsed
- **THEN** parsing fails with `Duplicate tokenizers defined for predicate 0-name`

#### Scenario: Two sortable tokenizers are refused
- **WHEN** `name: string @index(exact, exact) .` is parsed
- **THEN** parsing fails with `Only one index tokenizer can be sortable for 0-name`

#### Scenario: An index on uid is refused
- **WHEN** `friend: uid @index(exact) .` is parsed
- **THEN** parsing fails with `Indexing not allowed on predicate 0-friend of type uid`

#### Scenario: A tokenizer invalid for the type is refused
- **WHEN** `alive: bool @index(geo) .` is parsed
- **THEN** parsing fails with `Tokenizer: geo isn't valid for predicate: alive of type: bool`

#### Scenario: A list of bool is refused
- **WHEN** `friend: [bool] .` is parsed
- **THEN** parsing fails with `Unsupported type for list: [bool].`

#### Scenario: An unclosed list type is refused
- **WHEN** `friend: [string .` is parsed
- **THEN** parsing fails with `Unclosed [ while parsing schema for: friend`

#### Scenario: A type with duplicate fields is refused
- **WHEN** a type declares the field `name` twice
- **THEN** parsing fails with `Duplicate fields with name: name`

#### Scenario: Directives are checked against the type
- **WHEN** `@lang` is declared on an int predicate and `@reverse` on a string predicate
- **THEN** both are refused

### Requirement: Namespaces in schema text
A predicate prefixed with a namespace (`[0xf2] name: string .`) SHALL keep that namespace, and parsing for a
given namespace SHALL place every predicate in it.

#### Scenario: A namespace prefix is kept
- **WHEN** `[0xf2] name: string .` is parsed
- **THEN** the predicate is in namespace 0xf2

#### Scenario: Parsing for a namespace overrides it
- **WHEN** `name: string .` is parsed for namespace 5
- **THEN** the predicate is in namespace 5

### Requirement: Vector index declarations
`@index(hnsw(...))` on a `float32vector` predicate SHALL parse into a vector index specification with its
options, validated against the registry's hnsw factory.

#### Scenario: An hnsw index parses with its metric
- **WHEN** `emb: float32vector @index(hnsw(metric: "euclidean", exponent: "4")) .` is parsed
- **THEN** the update carries an hnsw vector index with metric euclidean and exponent 4

### Requirement: In-memory schema state
The schema state SHALL hold the current predicate and type definitions per namespace, SHALL answer the questions later
stages ask of it (value type, list, tokenizers, reverse, count, upsert, lang, noconflict and vector index), and SHALL
return a predicate's full update, `unique` included. The initial schema and types SHALL be Dgraph's, per namespace.

#### Scenario: The state answers for a parsed predicate
- **WHEN** `email: string @index(exact) @upsert @unique .` is loaded into the state
- **THEN** the state reports type string, tokenizer exact and upsert true for `email`, and its update carries unique
  true

#### Scenario: The initial schema is Dgraph's
- **WHEN** the complete initial schema and types are taken for namespaces 0 and 5
- **THEN** namespace 0's hold Dgraph's initial predicates and types, `dgraph.type`, the ACL entries and the
  `dgraph.graphql.*` entries among them, and namespace 5's lack `dgraph.namespace.name` and `dgraph.namespace.id`
