## ADDED Requirements

### Requirement: RDF N-Quads parse as Dgraph parses them
An RDF line SHALL parse into an N-Quad with subject, predicate, object id or value, value type, language,
facets and namespace label, equal to Dgraph's `chunker` RDF parser, and SHALL be refused where Dgraph refuses
it.

#### Scenario: Dgraph's RDF parser cases pass
- **WHEN** the 133 cases of Dgraph's RDF parser table are parsed
- **THEN** each gives Dgraph's N-Quad or Dgraph's error

#### Scenario: A typed literal takes its type
- **WHEN** `<a> <age> "013"^^<xs:int> .` is parsed
- **THEN** the object value is the int 13

#### Scenario: A language tag is kept
- **WHEN** `<a> <name> "chat"@fr .` is parsed
- **THEN** the N-Quad's language is `fr`

#### Scenario: The star form deletes all predicates
- **WHEN** `<alice> * * .` is parsed
- **THEN** the predicate and the object value are `_STAR_ALL`

#### Scenario: A missing object is refused
- **WHEN** `<alice> <knows> .` is parsed
- **THEN** parsing fails

### Requirement: JSON mutations parse into N-Quads
A JSON object or array SHALL parse into N-Quads as Dgraph's JSON parser does: `uid` names the node, nested
objects become edges, `|` separates a predicate from its facet, and a node without `uid` becomes a blank node.

#### Scenario: Nested JSON becomes edges
- **WHEN** `{"uid": "_:a", "name": "A", "friend": {"uid": "_:b", "name": "B"}}` is parsed
- **THEN** the result holds `name` for both nodes and a `friend` edge from `_:a` to `_:b`

#### Scenario: A facet travels with its predicate
- **WHEN** `{"uid": "_:a", "friend": {"uid": "_:b"}, "friend|close": true}` is parsed
- **THEN** the `friend` edge carries the facet `close` with bool value true

#### Scenario: A negative uid is refused
- **WHEN** a JSON object has `"uid": "-100"`
- **THEN** parsing fails

#### Scenario: A node without uid gets a blank node
- **WHEN** a JSON object without `uid` is parsed
- **THEN** it is assigned a blank node of the form `_:dg.<id>.<n>`, and two parses of one input differ only in
  `<id>`

### Requirement: N-Quads become directed edges
An N-Quad SHALL convert into a directed edge with entity, attribute, value or value id, value type, language,
facets and operation, as Dgraph's conversion does.

#### Scenario: A value N-Quad becomes a value edge
- **WHEN** `<0x1> <name> "A"@en .` is converted for a set
- **THEN** the edge has entity 0x1, attribute `name`, string value `A`, language `en` and operation SET
