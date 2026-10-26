# schema-storage Specification

## Purpose
How the schema persists in the store, and how a write to an unknown predicate creates its schema.
## Requirements
### Requirement: The schema persists in the store
The catalog SHALL be written to catalog keys with the discard flag and loaded from them at startup, per namespace, in
place of Dgraph's schema and type keys.

#### Scenario: The catalog survives a restart
- **WHEN** declarations are made and the store is closed and reopened
- **THEN** the loaded catalog equals the catalog before closing

#### Scenario: A removed declaration stays removed after a restart
- **WHEN** a predicate's declaration is removed and the store is reopened
- **THEN** the loaded catalog does not declare the predicate

### Requirement: Writes to an unknown predicate create its schema
A write to a predicate the catalog does not declare SHALL record a default entry for it, kind either, multi-valued, no
datatype constraint and no index, in the write's commit batch; a write that fails SHALL record none. Concurrent first
writes of one predicate SHALL record the same default entry, so neither depends on the other.

#### Scenario: A first write records the predicate
- **WHEN** `<a> <nickname> "Al"` is written and `nickname` is not declared
- **THEN** the catalog holds `nickname` with the default entry

#### Scenario: A failed write records no predicate
- **WHEN** a write naming `<alias>` fails before it commits and `alias` was not declared
- **THEN** the catalog does not hold `alias`

#### Scenario: Concurrent first writes both commit
- **WHEN** two writes read before either commits, the first writes `<a> <score> 12` and the second `<b> <score> "high"`,
  and `score` was not declared but already has a UID, as an object of an earlier statement (two writes creating one new
  IRI are the term dictionary's "The second creator retries")
- **THEN** both commit, and the catalog holds `score` with the default entry

### Requirement: A catalog entry that does not read back is corruption
A catalog key that does not parse, or a catalog entry that does not decode, SHALL refuse the load of its namespace's
catalog with the store's corruption error naming the key, skipping nothing: every entry is written from a declaration
through the catalog's encoding, so one that does not read back is a fault of the store or of the code, and a declaration
skipped would let writes go on under the default declaration. Dgraph skips such a schema key and logs it.

#### Scenario: A catalog key that does not parse refuses the load
- **WHEN** a namespace's catalog is loaded from a store holding a catalog key that does not parse
- **THEN** the load fails with a corruption error naming the key, and nothing panics

