## ADDED Requirements

### Requirement: A key ending after its attribute is refused
Parsing a key whose attribute takes its last byte, other than a schema or type key, SHALL fail with Dgraph's
`Invalid format for key` error, as every other truncation of a key does; Dgraph panics on it (ledger 6).

#### Scenario: A predicate prefix does not parse as a key
- **WHEN** the predicate prefix of `aa` in namespace 0 is parsed as a key
- **THEN** parsing fails with `Invalid format for key` and nothing panics
