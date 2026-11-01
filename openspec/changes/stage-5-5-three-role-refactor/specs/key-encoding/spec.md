## MODIFIED Requirements

### Requirement: Keys parse back into their parts
Parsing a key SHALL return its keyspace, namespace and the fields its keyspace defines: graph, predicate, subject,
object, tokenizer and token, direction and count, node, statement ID, or a reverse key's source-bucket part. Every ID in
a key SHALL be a 128-bit system ID written as its 16 big-endian bytes. A key whose IDs, other than the default graph's,
are invalid system IDs, or whose fields are shorter than their keyspace defines, SHALL be refused with an error naming
the key's bytes.

#### Scenario: A forward key round-trips
- **WHEN** a forward key for namespace 133, the default graph, the predicate with ID (type 1, bucket 7, sequence 7) and
  the subject with ID (type 1, bucket 42, sequence 42) is parsed
- **THEN** the result is a forward key with namespace 133, the default graph, and those predicate and subject IDs

#### Scenario: An invalid subject ID is refused
- **WHEN** a forward key carrying a subject ID whose sequence is 0 is parsed
- **THEN** parsing fails with an error naming the key's bytes

#### Scenario: A literal object round-trips
- **WHEN** a reverse key whose object is the encoding of `"Ann"@en` is built and parsed
- **THEN** the parsed object decodes to `"Ann"@en`, and its source-bucket part is 0 of 1

#### Scenario: A reverse key keeps its part
- **WHEN** a reverse key with source-bucket part 3 of 4 is built and parsed
- **THEN** the parse returns part 3 of 4

### Requirement: Split keys address the parts of a split posting list
A split key SHALL be the base key with the split flag set in its keyspace byte and the part's start appended: an ID as
its 16 big-endian bytes for lists of IDs, a 64-bit fingerprint for lists of literals. Only keys of list keyspaces SHALL
be split.

#### Scenario: A split key round-trips
- **WHEN** the split key for a forward key and the start ID (type 1, bucket 5, sequence 1000) is built and parsed
- **THEN** the parse returns the base key's parts and that start ID

#### Scenario: A split key with a trailing byte is refused
- **WHEN** one byte is appended to a valid split forward key and the result is parsed
- **THEN** parsing fails

### Requirement: A truncated key is refused
Parsing a truncated key SHALL fail with an error naming the key's bytes, and SHALL not panic. A truncated key is any
proper prefix of a valid key, other than one that is itself a valid key of a keyspace whose last field may be empty.

#### Scenario: Every prefix of a key fails cleanly
- **WHEN** every proper prefix of a forward, reverse, value-index and statement key is parsed
- **THEN** each parse fails with an error, and nothing panics

#### Scenario: Every prefix of a 128-bit ID field fails cleanly
- **WHEN** a forward key is cut inside its subject ID, at each of the ID's 15 inner positions, and parsed
- **THEN** each parse fails with an error naming the key's bytes
