## ADDED Requirements

### Requirement: Adding an index rebuilds it from existing data
Adding a tokenizer, `@reverse` or `@count` to a predicate holding data SHALL build the new entries from every
existing value at the change's timestamp, in the background, while writes keep both the old and the new entries;
until the rebuild completes, queries SHALL use an interim schema: the new schema without the indexes being built and
without the indexes being removed.

#### Scenario: A new index covers existing data
- **WHEN** a predicate holds 1,000 values and `@index(exact)` is added
- **THEN** after the rebuild, the index holds a token for each of the 1,000 values

#### Scenario: Queries use the interim schema during a rebuild
- **WHEN** a predicate indexed by `term` changes to `exact` and the `exact` index is being built
- **THEN** the schema state used for queries reports the predicate with neither `term` nor `exact`

#### Scenario: Writes during a rebuild are indexed
- **WHEN** a value is written while its predicate's index is rebuilt
- **THEN** after the rebuild, the index holds the value's tokens

### Requirement: Removing an index drops only its entries
Removing a tokenizer, `@reverse` or `@count` SHALL drop that index's keys, split parts included, and leave the others.

#### Scenario: Removing one tokenizer keeps another
- **WHEN** a predicate indexed by `term` and `exact` drops `term`
- **THEN** its term keys are gone and its exact keys remain

### Requirement: Type changes
Changing a scalar predicate to a list SHALL re-identify its existing values as list values. Changing a list to a
scalar, a predicate between uid and a scalar type while it holds data, or a predicate to or from `password` SHALL be
refused.

#### Scenario: A scalar becomes a list
- **WHEN** `tag: string .` with values becomes `tag: [string] .`
- **THEN** every existing value is read back as an element of the list

#### Scenario: A list cannot become a scalar
- **WHEN** `tag: [string] .` is changed to `tag: string .`
- **THEN** the change is refused

#### Scenario: A uid predicate holding data cannot become a string
- **WHEN** `friend: uid .` holds edges and is changed to `friend: string .`
- **THEN** the change is refused

### Requirement: A failed rebuild restores the previous schema and its indexes
A rebuild that fails SHALL restore the previous schema and SHALL leave every index the previous schema declares
complete. Dgraph restores the schema record only, after its index drops and list rewrites have run.

#### Scenario: A failed rebuild keeps the old index usable
- **WHEN** `term` is replaced by `exact` and the `exact` build fails
- **THEN** the schema declares `term` and every term key is present

### Requirement: Drops
Dropping a predicate SHALL remove its data, index, reverse, count and schema keys and its split parts; dropping a
namespace's data SHALL keep its schema and types; dropping everything in a namespace SHALL remove its data, schema
and type keys and write that namespace's initial schema and types at timestamp 1; dropping everything SHALL leave only
the root namespace's initial schema and types, written at timestamp 1; dropping a type SHALL delete its type key.

#### Scenario: Dropping a predicate removes its split parts
- **WHEN** a predicate whose lists are split is dropped
- **THEN** no key of the predicate remains, split parts included

#### Scenario: Dropping data keeps the schema
- **WHEN** a namespace's data is dropped
- **THEN** its predicates hold no data and its schema and types are unchanged

#### Scenario: Dropping a namespace's everything survives a restart
- **WHEN** everything in namespace 3 is dropped and the store is reopened
- **THEN** namespace 3 holds only its initial schema and types

#### Scenario: Dropping everything leaves the initial schema
- **WHEN** everything is dropped
- **THEN** the store holds the root namespace's initial schema and types, written at timestamp 1, and nothing else
