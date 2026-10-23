# mutation-input Specification

## Purpose
RDF and JSON mutation input parsed into N-Quads, with Dgraph's error texts.
## Requirements
### Requirement: RDF N-Quads parse as Dgraph parses them
An RDF line SHALL parse into an N-Quad with subject, predicate, object id or value, value type, language,
facets and namespace label, equal to Dgraph's `chunker` RDF parser, and SHALL be refused where Dgraph refuses
it.

#### Scenario: Dgraph's RDF parser cases pass
- **WHEN** the 130 cases of Dgraph's RDF parser table are parsed
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
- **WHEN** `{"uid": "_:a", "friend": {"uid": "_:b", "friend|close": true}}` is parsed
- **THEN** the `friend` edge carries the facet `close` with bool value true

#### Scenario: A negative uid is refused
- **WHEN** a JSON object has `"uid": "-100"`
- **THEN** parsing fails

#### Scenario: A node without uid gets a blank node
- **WHEN** a JSON object without `uid` is parsed
- **THEN** it is assigned `_:dg.<id>.<n>`, with `<id>` a random u32 drawn once per process and `<n>` a process-wide
  counter from 1

### Requirement: Malformed values are refused
An RDF typed literal that decodes to no geometry SHALL be refused with `invalid ObjectValue`, as a typed literal with no
value is; Dgraph panics on it (ledger 19). A JSON mutation SHALL be one JSON value: anything but whitespace after it
SHALL be refused with `invalid character 'x' after top-level value`, naming the character as Go's scanner does; Dgraph
keeps the last of several values or ignores trailing data (ledger 22).

#### Scenario: A null geometry literal is refused
- **WHEN** `<a> <p> "null"^^<geo:geojson> .` is parsed
- **THEN** parsing fails with `invalid ObjectValue` and nothing panics

#### Scenario: A second JSON document is refused
- **WHEN** `[{"name":"a"}] [{"name":"b"}]` is parsed as a set mutation
- **THEN** parsing fails with `invalid character '[' after top-level value`

#### Scenario: Trailing data after a JSON object is refused
- **WHEN** `{"name":"a"} xyz` is parsed as a set mutation
- **THEN** parsing fails with `invalid character 'x' after top-level value`

### Requirement: Every JSON value carries its key's language
A JSON key of the form `predicate@lang` SHALL give each N-Quad built under it the predicate and the language apart,
lists included. Dgraph splits the key for every value except list items (ledger 23).

#### Scenario: List items under a tagged key carry the language
- **WHEN** `{"tags@en": ["a", "b"]}` is parsed as a set mutation
- **THEN** both N-Quads have predicate `tags` and language `en`

### Requirement: JSON numbers decode as encoding/json decodes them
JSON numbers SHALL keep their text and decode as Go's `encoding/json` with `UseNumber` gives them, the outcomes
Dgraph's paired parser tests assert for both its parsers (ledger 24).

#### Scenario: Deleting a float zero deletes that value
- **WHEN** `{"uid": "0x1", "p": 0.0}` is parsed as a delete mutation
- **THEN** the N-Quad's object is the float 0, not `*`

#### Scenario: An integer beyond int64 is refused
- **WHEN** `{"uid": "0x1", "p": 27670116110564327426}` is parsed as a set mutation
- **THEN** parsing fails with `strconv.ParseInt: parsing "27670116110564327426": value out of range`

### Requirement: The format option decides the input format
A data format option of `rdf` or `json` SHALL decide the format; any other option SHALL leave it to the file's
extension, `.gz` removed. This is what both loaders' `--format` help states; Dgraph's code lets RDF from either source
win over JSON (ledger 21).

#### Scenario: The json option reads an rdf file as JSON
- **WHEN** the format of `a.rdf` with the option `json` is decided
- **THEN** it is JSON

