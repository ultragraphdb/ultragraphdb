## ADDED Requirements

### Requirement: The schema persists in the store
Predicate and type definitions SHALL be written to schema and type keys with the discard flag, and the schema
state SHALL be loaded from them at startup, per namespace.

#### Scenario: The schema survives a restart
- **WHEN** a schema is applied and the store is closed and reopened
- **THEN** the loaded schema state equals the state before closing

#### Scenario: A deleted predicate stays deleted after a restart
- **WHEN** a predicate's schema is deleted and the store is reopened
- **THEN** the loaded schema does not hold the predicate

### Requirement: Writes to an unknown predicate create its schema
A write to a predicate the schema does not hold SHALL add it with the type inferred from the value, as Dgraph
does, at the write's timestamp.

#### Scenario: A first write defines the predicate
- **WHEN** `<0x1> <nickname> "Al"` is written and `nickname` has no schema
- **THEN** the schema holds `nickname` with type default
