# scalar-types Specification

## Purpose
Dgraph's scalar types as the port holds them: type identifiers, conversions, binary encodings, value inference,
ordering, passwords and facet values.
## Requirements
### Requirement: Scalar type identifiers
The type identifiers SHALL equal Dgraph's: default 0, binary 1, int 2, float 3, bool 4, datetime 5, geo 6, uid 7,
password 8, string 9, object 10, bigfloat 11, float32vector 12, with schema names as in Dgraph's
`types/scalar_types.go`.

#### Scenario: Schema type names map to identifiers
- **WHEN** the names `int`, `datetime`, `uid`, `bigfloat` and `float32vector` are looked up
- **THEN** they map to identifiers 2, 5, 7, 11 and 12

### Requirement: Values encode byte-identically to Dgraph
Converting a value to binary SHALL produce Dgraph's bytes: int as 8 bytes little-endian two's complement, float
as 8 bytes little-endian IEEE-754, bool as one byte 0 or 1, string, default and password as their bytes,
datetime in Go's `time.MarshalBinary` layout, geo as little-endian WKB, bigfloat as the shortest `g` text, and
float32vector as little-endian f32 values.

#### Scenario: Every type's encoding matches its golden vector
- **WHEN** the golden values of each type are converted to binary
- **THEN** each result equals Dgraph's bytes

#### Scenario: A UTC datetime uses version 1 of Go's layout
- **WHEN** `2018-10-03T08:05:01Z` is converted to binary
- **THEN** the result is 15 bytes starting with `0x01` and ending in the offset `FF FF`

#### Scenario: Binary decodes back to the value
- **WHEN** each golden binary value is converted back to its type
- **THEN** the result equals the original value

### Requirement: Conversions between types follow Dgraph's rules
Conversions SHALL accept and refuse the same inputs as Dgraph's `types.Convert`, with its error texts.

#### Scenario: A float beyond int64 is refused
- **WHEN** the float `1e20` is converted to int
- **THEN** conversion fails with `Float out of int64 range`

#### Scenario: Not a number is refused as a float
- **WHEN** the string `NaN` is converted to float
- **THEN** conversion fails with `Got invalid value: NaN`

#### Scenario: Binary decodes to bool only from zero or one
- **WHEN** the bytes `[]`, `[0]`, `[1]` and `[2]` are converted to bool
- **THEN** the results are false, false and true, and `[2]` fails

#### Scenario: Floats format as Go's shortest G form
- **WHEN** the float 2.5e-7 is converted to string
- **THEN** the result is `2.5E-07`

### Requirement: Big floats parse and format as Go's math/big
A bigfloat value SHALL be parsed and formatted as Go's `big.Float` at 200 bits does, byte for byte: parsing as
`UnmarshalText` (base prefixes `0x`, `0b`, `0o`, underscores, `Inf`, and Go's rounding through a 264-bit power of five,
which is not always correctly rounded), and formatting as `MarshalText`, the shortest digits within Go's symmetric
half-ulp interval in `%g` layout.

#### Scenario: Big-float golden vectors match Go
- **WHEN** the golden big-float inputs are parsed and formatted: hand-picked, random, exact midpoints between adjacent
  200-bit values, and powers of two
- **THEN** every 200-bit value equals Go's and every text equals Go's `MarshalText`

#### Scenario: A power of two formats as Go shortens it
- **WHEN** 2^-1100 at 200 bits is formatted
- **THEN** the text is `7.3621518290228626754368661771449651176491350325096363488672e-332`

#### Scenario: Go's big-float syntax is accepted
- **WHEN** `1_000`, `0x1.8p1`, `Inf` and `NaN` are parsed as bigfloat
- **THEN** the first three give `1000`, `3` and `+Inf`, and `NaN` is refused

### Requirement: Datetime parsing accepts Dgraph's forms
Datetime parsing SHALL accept a year, a year and month, a date, RFC 3339, RFC 3339 without a zone, and
`2006-01-02 15:04:05 -0700 MST`, and SHALL refuse a zone offset out of range.

#### Scenario: Partial dates parse
- **WHEN** `2018`, `2018-10` and `2018-10-03` are parsed
- **THEN** each yields midnight UTC on the first day of the period it names

#### Scenario: An out-of-range zone offset is refused
- **WHEN** `2018-10-03T08:05:01+25:00` is parsed
- **THEN** parsing fails with `time zone offset hour out of range`

### Requirement: Value inference
Inferring a type from an untyped value SHALL follow Dgraph's order, the first matching shape deciding: bool (`true`,
`TRUE`, `True`, `false`, `FALSE`, `False`), datetime (four digits and `-`), int, float, then default.

#### Scenario: Untyped values infer their type
- **WHEN** `true`, `TRUE`, `t`, `2018`, `2018-10`, `1World` and `2.99792e8` are inferred
- **THEN** the types are bool, bool, default, int, datetime, default and float

### Requirement: Passwords are bcrypt hashes
A password SHALL be stored as a bcrypt hash at cost 10, SHALL be at least 6 bytes long, and SHALL verify against
hashes Dgraph wrote.

#### Scenario: A short password is refused
- **WHEN** the string `abc` is converted to password
- **THEN** conversion fails with `Password too short, i.e. should have at least 6 chars`

#### Scenario: A Dgraph-written hash verifies
- **WHEN** a golden password and its Go-generated bcrypt hash are checked
- **THEN** the check succeeds, and a different password fails it

### Requirement: Values order as Dgraph sorts them
Sorting values SHALL compare numerically across int and float, SHALL order other mismatched types by type
identifier, and SHALL order strings of a language by that language's collation.

#### Scenario: Ints and floats sort together
- **WHEN** the values `3`, `2.5` and `-1` of mixed int and float types are sorted
- **THEN** the order is `-1`, `2.5`, `3`

#### Scenario: Language strings sort by collation
- **WHEN** the golden German strings are sorted with language `de`
- **THEN** the order equals Dgraph's

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
with `Value is not valid UTF-8 for type string`. Go Dgraph stores such bytes and tokenizes them inconsistently (ledger
17, 18, 31).

#### Scenario: Bytes that are not UTF-8 do not become a string
- **WHEN** the binary value `e6 97 ff` is converted to string
- **THEN** conversion fails with `Value is not valid UTF-8 for type string`

#### Scenario: Valid UTF-8 bytes become a string
- **WHEN** the binary value of `日本` is converted to string
- **THEN** the string is `日本`

