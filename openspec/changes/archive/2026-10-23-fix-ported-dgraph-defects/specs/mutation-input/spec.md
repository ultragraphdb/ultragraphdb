## ADDED Requirements

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
