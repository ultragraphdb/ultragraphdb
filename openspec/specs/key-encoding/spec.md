# key-encoding Specification

## Purpose
The byte layout of Dgraph's keys, as the port writes and parses them: data, index, reverse, count, schema, type
and split keys, namespaced attribute names, and the reserved and pre-defined predicates.
## Requirements
### Requirement: Keys follow Dgraph's byte layout
A key SHALL be `prefix (1 byte) | namespace (u64 big-endian) | attribute length (u16 big-endian) | attribute` followed
by the key kind's suffix, byte-identical to Dgraph's `x/keys.go`. Prefixes: default `0x00`, schema `0x01`, type `0x02`,
split `0x04`. Suffixes: data `0x00 | uid (u64 BE)`, index `0x02 | term`, reverse `0x04 | uid (u64 BE)`, count
`0x08 | count (u32 BE)`, reverse count `0x0C | count (u32 BE)`; schema and type keys carry none.

#### Scenario: A data key matches Dgraph's bytes
- **WHEN** the data key for attribute `name` in namespace 0 and uid 1 is built
- **THEN** it equals `00 | 00 00 00 00 00 00 00 00 | 00 04 | 6E 61 6D 65 | 00 | 00 00 00 00 00 00 00 01`

#### Scenario: Every key kind matches its golden vector
- **WHEN** data, index, reverse, count, reverse-count, schema and type keys are built for the golden inputs
- **THEN** each equals the bytes Dgraph produces for the same input

#### Scenario: Data keys of one predicate sort by uid
- **WHEN** data keys for one attribute are built for uids 1, 2, 256 and 2^40
- **THEN** their byte order equals their numeric uid order

### Requirement: Keys parse back into their parts
Parsing a key SHALL return its kind, namespace, attribute and suffix fields, as Dgraph's `x.Parse` does. It SHALL
reject a key shorter than 9 bytes, a data or reverse key whose uid is shorter than 8 bytes or is 0, a count key with
fewer than 4 bytes of count, and a split data, reverse or count key whose suffix is not exactly its fixed length; it
SHALL ignore bytes after an unsplit suffix, let an index term take every byte after `0x02`, and parse a key with
prefix `0xFF` to an empty key without error.

#### Scenario: A data key round-trips
- **WHEN** a data key for namespace 133, attribute `name` and uid 42 is parsed
- **THEN** the result is a data key with namespace 133, attribute `name` and uid 42

#### Scenario: A uid of zero is rejected
- **WHEN** a data key carrying uid 0 is parsed
- **THEN** parsing fails with `Invalid UID with value 0 for key:` followed by the key's bytes after its prefix and
  namespace, as a decimal byte list

#### Scenario: An attribute containing a hyphen round-trips
- **WHEN** a key for attribute `first-name` in namespace 7 is built and parsed
- **THEN** the parsed attribute is `first-name` and the namespace is 7

### Requirement: Split keys address the parts of a split posting list
A split key SHALL be the base key with its first byte set to `0x04` and the part's start uid appended as a u64
big-endian. Only keys with the default prefix SHALL be split.

#### Scenario: A split key round-trips
- **WHEN** the split key for a data key and start uid 1000 is built and parsed
- **THEN** the parse returns the base key's parts and start uid 1000

#### Scenario: A split data key with a trailing byte is rejected
- **WHEN** one byte is appended to a valid split data key and the result is parsed
- **THEN** parsing fails

### Requirement: Namespaced attribute names
An attribute held in memory SHALL be named `hex(namespace) + "-" + predicate`, and parsing that name SHALL return the
namespace and the predicate.

#### Scenario: A namespaced attribute is formatted in hexadecimal
- **WHEN** namespace 133 and predicate `name` are combined
- **THEN** the result is `85-name`, and parsing it returns namespace 133 and predicate `name`

### Requirement: Reserved and pre-defined predicates
A predicate whose name starts with `dgraph.`, ignoring case, SHALL be reserved. The pre-defined predicates SHALL be
`dgraph.type`, the six ACL predicates, `dgraph.drop.op`, `dgraph.graphql.schema`, `dgraph.graphql.xid`,
`dgraph.graphql.p_query`, `dgraph.namespace.id` and `dgraph.namespace.name`. Both checks SHALL take a namespaced
attribute.

#### Scenario: A reserved name need not be pre-defined
- **WHEN** `0-dgraph.blah` and `0-dgraph.type` are checked
- **THEN** both are reserved, and only `0-dgraph.type` is pre-defined

### Requirement: Registered reserved namespaces
A plugin SHALL be able to claim predicates and types under `dgraph.` by prefix or by exact name, and to lock
predicates' values to a trust marker by exact name or by prefix, as Dgraph's `RegisterReservedNamespace` allows; names
SHALL be bare and owned once; for a value lock an exact name SHALL win over a prefix and the longest prefix over shorter
ones; a conflicting registration SHALL be refused with Dgraph's text, where Dgraph panics.

#### Scenario: The longest prefix owns a name
- **WHEN** plugins lock the values of the prefixes `dgraph.acme.` and `dgraph.acme.billing.` to two different markers
- **THEN** `dgraph.acme.billing.total` is locked to the second marker and `dgraph.acme.users` to the first

#### Scenario: Registering a name twice is refused
- **WHEN** two plugins register the exact name `dgraph.acme.key`
- **THEN** the second registration is refused with `x.RegisterReservedNamespace: predicate "dgraph.acme.key" already
  registered`

