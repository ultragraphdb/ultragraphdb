## ADDED Requirements

### Requirement: Adding an index rebuilds it from existing data
Adding a tokenizer, `@reverse` or `@count` to a predicate holding data SHALL build the new entries from every
existing value at the change's timestamp, in the background, while writes keep both the old and the new entries;
queries SHALL use the old schema until the rebuild completes.

#### Scenario: A new index covers existing data
- **WHEN** a predicate holds 1,000 values and `@index(exact)` is added
- **THEN** after the rebuild, the index holds a token for each of the 1,000 values

#### Scenario: Queries use the old schema during a rebuild
- **WHEN** an index is being rebuilt
- **THEN** the schema state used for queries reports the predicate without the new index

#### Scenario: Writes during a rebuild are indexed
- **WHEN** a value is written while its predicate's index is rebuilt
- **THEN** after the rebuild, the index holds the value's tokens

### Requirement: Removing an index drops only its entries
Removing a tokenizer, `@reverse` or `@count` SHALL drop that index's keys, split parts included, and leave the
others.

#### Scenario: Removing one tokenizer keeps another
- **WHEN** a predicate indexed by `term` and `exact` drops `term`
- **THEN** its term keys are gone and its exact keys remain

### Requirement: List type changes
Changing a scalar predicate to a list SHALL re-identify its existing values as list values; changing a list to a
scalar SHALL be refused.

#### Scenario: A scalar becomes a list
- **WHEN** `tag: string .` with values becomes `tag: [string] .`
- **THEN** every existing value is read back as an element of the list

#### Scenario: A list cannot become a scalar
- **WHEN** `tag: [string] .` is changed to `tag: string .`
- **THEN** the change is refused

### Requirement: A failed rebuild restores the previous schema
A rebuild that fails SHALL leave the schema as it was before the change.

#### Scenario: The schema is restored after a failed rebuild
- **WHEN** an index rebuild fails part way
- **THEN** the schema state equals the state before the change

### Requirement: Drops
Dropping a predicate SHALL remove its data, index, reverse, count and schema keys and its split parts; dropping a
namespace's data SHALL keep its schema and types; dropping everything SHALL leave only the initial schema, at
timestamp 1.

#### Scenario: Dropping a predicate removes its split parts
- **WHEN** a predicate whose lists are split is dropped
- **THEN** no key of the predicate remains, split parts included

#### Scenario: Dropping data keeps the schema
- **WHEN** a namespace's data is dropped
- **THEN** its predicates hold no data and its schema and types are unchanged

#### Scenario: Dropping everything leaves the initial schema
- **WHEN** everything is dropped
- **THEN** the store holds the initial schema and types, written at timestamp 1, and nothing else
