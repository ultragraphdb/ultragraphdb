## ADDED Requirements

### Requirement: Alter applies schema changes
`Alter` with schema text SHALL apply it as the `schema-changes` capability specifies, in the background when
`run_in_background` is set, and queries SHALL reflect an added or removed index once it is built or dropped. `Alter`
SHALL be refused while an index is being built, as Dgraph refuses it.

#### Scenario: An added index serves eq
- **WHEN** `Alter` adds `@index(exact)` to `name` and the rebuild completes
- **THEN** `eq(name, "A")` at the root succeeds

#### Scenario: A removed index no longer serves eq
- **WHEN** `Alter` removes the index of `name`
- **THEN** `eq(name, "A")` at the root fails as unindexed

### Requirement: Alter drops
`drop_all` SHALL remove all data and schema and recreate the groot user and the guardians group; `drop_op` `DATA` SHALL
keep the schema and re-create groot and guardians; `ATTR` SHALL drop one predicate and `TYPE` one type, neither of them
pre-defined. With ACL on, `drop_all` SHALL require the superadmin and `DATA` a guardian of the namespace, and other
callers SHALL receive Dgraph's error `only guardians are allowed to drop all data, but the current user is <user>`;
`drop_all` SHALL be refused when the deployment disallows drops.

#### Scenario: drop_all recreates groot
- **WHEN** the superadmin calls `Alter` with `drop_all`
- **THEN** all data and schema are gone, and groot can log in with the default password

#### Scenario: Dropping data keeps the schema through the API
- **WHEN** `Alter` runs with `drop_op` `DATA`
- **THEN** queries return no data and the schema query returns the previous schema

#### Scenario: Dropping a type keeps its predicates
- **WHEN** `Alter` drops type `Person`
- **THEN** the type is gone and its predicates remain

#### Scenario: Dropping a pre-defined predicate is refused
- **WHEN** `Alter` drops the predicate `dgraph.type`
- **THEN** it is refused

#### Scenario: drop_all by a non-guardian is refused
- **WHEN** ACL is on and a user outside guardians calls `Alter` with `drop_all`
- **THEN** it is refused with `only guardians are allowed to drop all data, but the current user is` and the user
