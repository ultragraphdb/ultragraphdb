## ADDED Requirements

### Requirement: Measures, precisions and bfloat16 vectors are datatypes
The datatypes `ugdb:measure`, `ugdb:precision` and `ugdb:bfloat16vector` SHALL be known datatypes with the codes,
tags and by-value payloads `PROTOCOL.md` sections 10.1, 10.4 and 10.5 give them. A `ugdb:measure` literal's lexical
form SHALL be a number in `xsd:double`'s canonical form, one space and a UCUM code; one that does not parse SHALL be
refused as a malformed literal. A `ugdb:precision` literal SHALL be one of `millennium`, `century`, `decade`, `year`,
`month`, `day`, `hour`, `minute`, `second`, `millisecond`, `microsecond` and `nanosecond`. A `ugdb:bfloat16vector`
literal SHALL hold its elements as bfloat16 and convert to and from text by its canonical form. Each SHALL round-trip
through its encoding byte for byte.

#### Scenario: A measure literal round-trips
- **WHEN** the literal `"35000 [ft_i]"^^ugdb:measure` is encoded and decoded
- **THEN** the decoded literal equals it, with the value 35000 and the unit code `[ft_i]`

#### Scenario: A measure without a unit is malformed
- **WHEN** the literal `"35000"^^ugdb:measure` is encoded
- **THEN** it is refused as a malformed `ugdb:measure`

#### Scenario: An unknown precision is refused
- **WHEN** the literal `"fortnight"^^ugdb:precision` is encoded
- **THEN** it is refused as a malformed `ugdb:precision`
