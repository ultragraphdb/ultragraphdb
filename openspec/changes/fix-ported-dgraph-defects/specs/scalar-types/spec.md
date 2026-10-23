## ADDED Requirements

### Requirement: Floats outside int64 are refused as ints and as datetimes
Converting a float or a bigfloat to an int or to a datetime SHALL refuse every value outside int64, NaN and the
infinities with `Float out of int64 range`. A datetime takes the value as seconds. Dgraph's guard lets exactly 2^63
through to an int, and its datetime conversion has none; both give results that differ between amd64 and arm64
(ledger 10).

#### Scenario: Two to the 63 is not an int
- **WHEN** the float `9223372036854775808` is converted to int
- **THEN** conversion fails with `Float out of int64 range`

#### Scenario: A float beyond int64 seconds is not a datetime
- **WHEN** the floats `1e19`, `+Inf` and `NaN` and the bigfloat `1e19` are each converted to datetime
- **THEN** each conversion fails with `Float out of int64 range`

### Requirement: String values are valid UTF-8
A value converted to the string or default type SHALL be valid UTF-8. Bytes that are not valid UTF-8 SHALL be refused
with `Value is not valid UTF-8 for type string`. Every mutation path converts its values to the predicate's schema type,
so a mutation carrying such a value is refused when it arrives. Go Dgraph stores such bytes and tokenizes them
inconsistently (ledger 17, 18, 31).

#### Scenario: Bytes that are not UTF-8 do not become a string
- **WHEN** the binary value `e6 97 ff` is converted to string
- **THEN** conversion fails with `Value is not valid UTF-8 for type string`

#### Scenario: Valid UTF-8 bytes become a string
- **WHEN** the binary value of `日本` is converted to string
- **THEN** the string is `日本`
