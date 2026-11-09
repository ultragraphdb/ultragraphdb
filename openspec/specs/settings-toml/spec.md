# settings-toml Specification

## Purpose
The settings files' format: the subset of TOML 1.0 the roles' settings use, parsed in-house by `ultragraphdb_tool_toml`
in one bounded pass refusing by name any feature outside the subset, and read through typed accessors refusing any key
a role leaves unread, so a file means the same to every TOML reader and a misspelled setting stops the start.

## Requirements
### Requirement: Settings files are a subset of TOML, parsed in-house
`ultragraphdb_tool_toml` SHALL parse the subset of TOML 1.0 the roles' settings use, and nothing more: blank lines;
comments from `#` to the end of a line, alone or after a value; `[table]` headers with a bare name; keys before any
header, in the root table; `key = value` lines with a bare key (`A–Z`, `a–z`, `0–9`, `_`, `-`); and three value kinds:
basic strings in double quotes with TOML's escapes (`\b`, `\t`, `\n`, `\f`, `\r`, `\"`, `\\`, `\uXXXX`, `\UXXXXXXXX`),
decimal integers with an optional sign and `_` between digits, fitting in an `i64`, and the booleans `true` and
`false`. A document the subset accepts SHALL parse to the same tables, keys and values a conforming TOML 1.0 parser
gives it. The crate SHALL have no external dependency.

#### Scenario: A settings file parses
- **WHEN** the document `name = "dev"`, `[storage]`, `partition_count = 4  # one per core`, `sync = true` is parsed
- **THEN** the root table holds `name` = `"dev"`, and the table `storage` holds `partition_count` = 4 and `sync` = true

#### Scenario: Escapes and integers follow TOML
- **WHEN** the values `"a\tbé"`, `-1_000` and `+7` are parsed
- **THEN** they are the string `a`, tab, `b`, `é`, and the integers −1000 and 7

#### Scenario: The subset agrees with TOML's own test suite
- **WHEN** every document of the toml-test suite's valid cases that uses only the subset is parsed
- **THEN** each parses to the values the suite's expected JSON states, the documents vendored under `testdata/` with
  their source and version

### Requirement: Anything outside the subset is refused by name
A document using any TOML feature outside the subset SHALL be refused with an error naming the feature and its line,
never parsed in part and never read as something else: dotted keys, quoted keys, literal strings, multi-line strings,
floats, infinities and NaN, offset and local dates and times, arrays, inline tables, arrays of tables, hexadecimal,
octal and binary integers. Every document TOML 1.0 itself refuses SHALL be refused too: a duplicate key, a duplicate
table, an integer with a leading zero or out of range, an invalid escape, a key without a value, and a line with
anything after its value but a comment.

#### Scenario: An array is refused as unsupported
- **WHEN** a document holds `peers = ["a", "b"]` on line 3
- **THEN** parsing fails with an unsupported-feature error naming arrays and line 3

#### Scenario: A float is refused as unsupported
- **WHEN** a document holds `ratio = 0.5`
- **THEN** parsing fails with an unsupported-feature error naming floats and the line

#### Scenario: A duplicate key is refused as invalid
- **WHEN** a table holds `sync = true` twice
- **THEN** parsing fails with an invalid-document error naming `sync` and the second line

#### Scenario: TOML's invalid documents are refused
- **WHEN** every document of the toml-test suite's invalid cases is parsed
- **THEN** each is refused, as unsupported or as invalid, and none parses

### Requirement: Reading settings consumes every key
A document SHALL be read through typed accessors that take a table and a key and answer the value or an error naming
the table, the key and the kind expected; after a role has read its settings, any key it did not take SHALL be refused
with an error naming the table and the key, so a misspelled setting stops the start.

#### Scenario: A misspelled key is refused
- **WHEN** a role reads `[storage] partition_count` from a document holding `[storage] partiton_count = 4`
- **THEN** the read of `partition_count` reports it missing, and the final check refuses `storage.partiton_count`

#### Scenario: A value of the wrong kind is refused
- **WHEN** a role reads `[storage] partition_count` as an integer from `partition_count = "4"`
- **THEN** the read fails with an error naming `storage.partition_count`, an integer expected and a string found

### Requirement: The parser is bounded
The parser SHALL refuse a document over 64 KiB, a line over 4 KiB, a table count over 256 and a key count over 4,096,
each with an error naming the limit; it SHALL read the document in one pass with no recursion, and SHALL never panic on
any input.

#### Scenario: An oversized document is refused
- **WHEN** a document of 64 KiB plus one byte is parsed
- **THEN** parsing fails with an error naming the 64 KiB limit

#### Scenario: Arbitrary bytes never panic
- **WHEN** the fuzzer feeds the parser arbitrary bytes
- **THEN** every input is accepted or refused with a typed error, and nothing panics

