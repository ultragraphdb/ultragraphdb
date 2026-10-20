Feature: Schema parsing
  Schema text parses into predicate and type updates equal to Dgraph's schema.Parse output, invalid schemas are
  refused with Dgraph's error texts, namespaces are kept or overridden, vector indexes carry their options, and the
  in-memory schema state answers for what it holds and starts from Dgraph's initial schema and types.

  Scenario: A uid list predicate parses
    When the schema "friend: [uid] ." is parsed
    Then the predicate is "0-friend" of value type 7 and a list

  Scenario: Parsed schemas match Dgraph's updates
    When each schema of the golden schema corpus is parsed
    Then the updates equal Dgraph's

  Scenario: A duplicate tokenizer is refused
    When the schema "name: string @index(term, term) ." is parsed
    Then parsing fails with "Duplicate tokenizers defined for predicate 0-name"

  Scenario: Two sortable tokenizers are refused
    When the schema "name: string @index(exact, exact) ." is parsed
    Then parsing fails with "Only one index tokenizer can be sortable for 0-name"

  Scenario: An index on uid is refused
    When the schema "friend: uid @index(exact) ." is parsed
    Then parsing fails with "Indexing not allowed on predicate 0-friend of type uid"

  Scenario: A tokenizer invalid for the type is refused
    When the schema "alive: bool @index(geo) ." is parsed
    Then parsing fails with "Tokenizer: geo isn't valid for predicate: alive of type: bool"

  Scenario: A list of bool is refused
    When the schema "friend: [bool] ." is parsed
    Then parsing fails with "Unsupported type for list: [bool]."

  Scenario: An unclosed list type is refused
    When the schema "friend: [string ." is parsed
    Then parsing fails with "Unclosed [ while parsing schema for: friend"

  Scenario: A type with duplicate fields is refused
    When a type declaring the field "name" twice is parsed
    Then parsing fails with "Duplicate fields with name: name"

  Scenario: Directives are checked against the type
    When the schema "age: int @lang ." is parsed
    Then parsing fails with "@lang directive can only be specified for string type. Got: [int] for attr: [0-age]"
    When the schema "name: string @reverse ." is parsed
    Then parsing fails with "Cannot reverse for non-UID type"

  Scenario: A namespace prefix is kept
    When the schema "[0xf2] name: string ." is parsed
    Then the predicate is "f2-name" in namespace 0xf2

  Scenario: Parsing for a namespace overrides it
    When the schema "name: string ." is parsed for namespace 5
    Then the predicate is "5-name" in namespace 5

  Scenario: An hnsw index parses with its metric
    When the schema 'emb: float32vector @index(hnsw(metric: "euclidean", exponent: "4")) .' is parsed
    Then the update carries an hnsw vector index with metric "euclidean" and exponent "4"

  Scenario: The state answers for a parsed predicate
    When the schema "email: string @index(exact) @upsert @unique ." is loaded into the state
    Then the state reports type string, tokenizer "exact" and upsert true for "0-email"
    And the update of "0-email" carries unique true

  Scenario: The initial schema is Dgraph's
    When the complete initial schema and types are taken for namespaces 0 and 5
    Then they equal Dgraph's, with "dgraph.type", the ACL entries and the "dgraph.graphql.*" entries in namespace 0
    And namespace 5's lack "dgraph.namespace.name" and "dgraph.namespace.id"
