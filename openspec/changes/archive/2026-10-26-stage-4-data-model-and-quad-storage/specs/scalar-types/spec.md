## MODIFIED Requirements

### Requirement: String values are valid UTF-8
A value converted to the string or default type SHALL be valid UTF-8. Bytes that are not valid UTF-8 SHALL be refused
with `Value is not valid UTF-8 for type string`. Go Dgraph stores such bytes and tokenizes them inconsistently (ledger
17, 18, 31).

#### Scenario: Bytes that are not UTF-8 do not become a string
- **WHEN** the binary value `e6 97 ff` is converted to string
- **THEN** conversion fails with `Value is not valid UTF-8 for type string`

#### Scenario: Valid UTF-8 bytes become a string
- **WHEN** the binary value of `日本` is converted to string
- **THEN** the string is `日本`

## REMOVED Requirements

### Requirement: Facet values
**Reason**: Facets are DQL's attributes of an edge; the mapping contract states them as properties of a statement,
whose values are RDF literals the term dictionary encodes.
**Migration**: A statement's properties, through the changeset's statement-property sets (`mapping-contract`).
