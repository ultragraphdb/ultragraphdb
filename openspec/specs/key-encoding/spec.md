# key-encoding Specification

## Purpose
The byte layout of Dgraph's keys, as the port writes and parses them: data, index, reverse, count, schema, type
and split keys, namespaced attribute names, and the reserved and pre-defined predicates.
## Requirements
### Requirement: Keys parse back into their parts
Parsing a key SHALL return its keyspace, namespace and the fields its keyspace defines: graph, predicate, subject,
object, tokenizer and token, direction and count, node, or statement ID. A key whose UIDs, other than the default
graph's, are 0, or whose fields are shorter than their keyspace defines, SHALL be refused with an error naming the key's
bytes.

#### Scenario: A forward key round-trips
- **WHEN** a forward key for namespace 133, the default graph, predicate UID 7 and subject UID 42 is parsed
- **THEN** the result is a forward key with namespace 133, graph 0, predicate 7 and subject 42

#### Scenario: A subject UID of zero is refused
- **WHEN** a forward key carrying subject UID 0 is parsed
- **THEN** parsing fails with an error naming the key's bytes

#### Scenario: A literal object round-trips
- **WHEN** a reverse key whose object is the encoding of `"Ann"@en` is built and parsed
- **THEN** the parsed object decodes to `"Ann"@en`

### Requirement: Split keys address the parts of a split posting list
A split key SHALL be the base key with the split flag set in its keyspace byte and the part's start appended: a UID as
a u64 big-endian for lists of UIDs, a 64-bit fingerprint for lists of literals. Only keys of list keyspaces SHALL be
split.

#### Scenario: A split key round-trips
- **WHEN** the split key for a forward key and start 1000 is built and parsed
- **THEN** the parse returns the base key's parts and start 1000

#### Scenario: A split key with a trailing byte is refused
- **WHEN** one byte is appended to a valid split forward key and the result is parsed
- **THEN** parsing fails

### Requirement: A truncated key is refused
Parsing a truncated key SHALL fail with an error naming the key's bytes, and SHALL not panic. A truncated key is any
proper prefix of a valid key, other than one that is itself a valid key of a keyspace whose last field may be empty.

#### Scenario: Every prefix of a key fails cleanly
- **WHEN** every proper prefix of a forward, reverse, value-index and statement key is parsed
- **THEN** each parse fails with an error, and nothing panics

