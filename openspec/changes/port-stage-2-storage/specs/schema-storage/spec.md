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
A write to a predicate the schema does not hold SHALL add it to a schema overlay local to the request, with the type
inferred from the value, a new uid predicate as `[uid]`, and a `SINGLE` or `LIST` hint overriding the list flag, and
SHALL write it in the request's commit batch at its commit timestamp; a request that fails SHALL create no schema. At
commit, under the writer lock, a request whose new predicate gained a schema since its read SHALL convert its values to
that schema or fail with Dgraph's conversion error, so the first commit's schema wins. Dgraph writes the schema at the
start timestamp and keeps it after an abort.

#### Scenario: A first write defines the predicate
- **WHEN** `<0x1> <nickname> "Al"` is written and `nickname` has no schema
- **THEN** the schema holds `nickname` with type default

#### Scenario: A failed write defines no predicate
- **WHEN** a request writing `<0x1> <alias> "Al"` fails before it commits and `alias` had no schema
- **THEN** the schema does not hold `alias`

#### Scenario: A new uid predicate is a list
- **WHEN** `<0x1> <knows> <0x2>` is written and `knows` has no schema
- **THEN** the schema holds `knows: [uid]`

#### Scenario: Concurrent first writes keep the first commit's type
- **WHEN** two requests read before either commits, the first writes `<0x1> <score> "12"^^<xs:int>` and commits, and
  the second writes `<0x2> <score> "high"` and commits after it
- **THEN** the schema holds `score: int`, and the second commit fails with `strconv.ParseInt: parsing "high": invalid
  syntax` and writes nothing
