## ADDED Requirements

### Requirement: Adding an index rebuilds it from existing data
Adding a tokenizer, `@reverse` or `@count` to a predicate holding data SHALL build the new entries from every
existing value at the change's timestamp, in the background, while writes maintain the previous schema's indexes as
well as the new schema's; Dgraph maintains only the new schema's. When the build succeeds, one atomic batch SHALL swap
the indexes: for every key under a rebuilt prefix, the built list, or a deletion where the build produced none, at the
rebuild's timestamp; for every key under a removed prefix, a deletion above every write that maintained it; the
list-type rewrite and a `@count` rebuild's data lists; and the new schema record. Until the rebuild completes, queries
SHALL use an interim schema: the new schema without the indexes being built and without the indexes being removed.

#### Scenario: A new index covers existing data
- **WHEN** a predicate holds 1,000 values and `@index(exact)` is added
- **THEN** after the rebuild, the index holds a token for each of the 1,000 values

#### Scenario: Queries use the interim schema during a rebuild
- **WHEN** a predicate indexed by `term` changes to `exact` and the `exact` index is being built
- **THEN** the schema state used for queries reports the predicate with neither `term` nor `exact`

#### Scenario: Writes during a rebuild are indexed
- **WHEN** a value is written while its predicate's index is rebuilt
- **THEN** after the rebuild, the index holds the value's tokens

#### Scenario: Writes during a rebuild leave no entry of a removed index
- **WHEN** `name: string @index(term) .` changes to `name: string @index(exact) .`, and `name` of `0x2` is set to
  `pear` while the `exact` index is built
- **THEN** after the rebuild, no term key of `name` remains, and the exact key for `pear` contains `0x2`

#### Scenario: A value-type change rebuilds its tokens whole
- **WHEN** `n: string @index(exact) .` holding `7` for `0x1` changes to `n: int @index(int) .`, and `n` of `0x1` is set
  to `8` while the index is built
- **THEN** after the rebuild, no exact key of `n` remains, and the int keys of `n` list `0x1` under 8 and not under 7

### Requirement: Removing an index drops only its entries
Removing a tokenizer, `@reverse` or `@count` SHALL drop that index's keys, split parts included, and leave the others.

#### Scenario: Removing one tokenizer keeps another
- **WHEN** a predicate indexed by `term` and `exact` drops `term`
- **THEN** its term keys are gone and its exact keys remain

### Requirement: Type changes
Changing a scalar predicate to a list SHALL re-identify its existing values as list values. Changing a list to a scalar
SHALL be refused, with `Schema change not allowed from [<old>] => <new> without deleting pred: <attr>` while the
predicate holds data and with `Type can't be changed from list to scalar for attr: [<attr>] without dropping it first.`
otherwise; changing a predicate between uid and a scalar type while it holds data SHALL be refused with `Schema change
not allowed from scalar to uid or vice versa while there is data for pred: <attr>`; changing a scalar predicate to or
from `password` SHALL be refused with `Schema change not allowed from <old> to <new>`. A predicate holds data when one
of its data lists is non-empty at the change's timestamp; Dgraph's `hasEdges` counts any key whose newest version
is not empty. A refused change SHALL alter nothing; Dgraph refuses a list-to-scalar change on an empty predicate after
its indexes are dropped.

#### Scenario: A scalar becomes a list
- **WHEN** `tag: string .` with values becomes `tag: [string] .`
- **THEN** every existing value is read back as an element of the list

#### Scenario: A list cannot become a scalar
- **WHEN** `tag: [string] .` holding values is changed to `tag: string .`
- **THEN** the change fails with `Schema change not allowed from [string] => string without deleting pred: tag`

#### Scenario: An empty list cannot become a scalar
- **WHEN** `tag: [string] @index(exact) .` holding no values is changed to `tag: string .`
- **THEN** the change fails with `Type can't be changed from list to scalar for attr: [tag] without dropping it
  first.`, and the schema still declares `tag: [string] @index(exact) .`

#### Scenario: A uid predicate holding data cannot become a string
- **WHEN** `friend: uid .` holds edges and is changed to `friend: string .`
- **THEN** the change fails with `Schema change not allowed from scalar to uid or vice versa while there is data for
  pred: friend`

#### Scenario: An empty uid predicate can become a password
- **WHEN** `token: uid .` holds no edges and is changed to `token: password .`
- **THEN** the change is applied

#### Scenario: Deltas summing to nothing hold no data
- **WHEN** `friend: uid .` gained the edge `0x1 → 0x2` at 3 and lost it at 4, and is changed to `friend: string .` at 5
- **THEN** the change is applied

#### Scenario: A string cannot become a password
- **WHEN** `secret: string .` is changed to `secret: password .`
- **THEN** the change fails with `Schema change not allowed from STRING to PASSWORD`

### Requirement: A failed rebuild restores the previous schema and its indexes
A rebuild that fails SHALL restore the previous schema and SHALL leave every index the previous schema declares
complete. Dgraph restores the schema record only, after its index drops and list rewrites have run.

#### Scenario: A failed rebuild keeps the old index usable
- **WHEN** `term` is replaced by `exact` and the `exact` build fails
- **THEN** the schema declares `term` and every term key is present

### Requirement: Drops
Dropping a predicate SHALL remove its data, index, reverse, count and schema keys and its split parts, and for a vector
predicate its `__vector_entry`, `__vector_` and `__vector_dead` predicates; dropping a namespace's data SHALL keep its
schema and types; dropping everything in a namespace SHALL remove its data, schema and type keys and write that
namespace's initial schema and types at timestamp 1; dropping everything SHALL leave only the root namespace's initial
schema and types, written at timestamp 1; dropping a type SHALL delete its type key. Every drop SHALL remove what it
drops physically, so that the initial schema and types written at timestamp 1 afterwards are visible.

#### Scenario: Dropping a predicate removes its split parts
- **WHEN** a predicate whose lists are split is dropped
- **THEN** no key of the predicate remains, split parts included

#### Scenario: Dropping a vector predicate drops its index predicates
- **WHEN** `emb: float32vector @index(hnsw) .` holds vectors and is dropped
- **THEN** no key of `emb`, `emb__vector_entry`, `emb__vector_` or `emb__vector_dead` remains

#### Scenario: Dropping data keeps the schema
- **WHEN** a namespace's data is dropped
- **THEN** its predicates hold no data and its schema and types are unchanged

#### Scenario: Dropping a namespace's everything survives a restart
- **WHEN** everything in namespace 3 is dropped and the store is reopened
- **THEN** namespace 3 holds only its initial schema and types

#### Scenario: A namespace's initial schema is visible after its drop
- **WHEN** everything in namespace 3 is dropped at 50
- **THEN** a read at 60 finds namespace 3's initial schema and types

#### Scenario: Dropping everything leaves the initial schema
- **WHEN** everything is dropped
- **THEN** the store holds the root namespace's initial schema and types, written at timestamp 1, and nothing else

### Requirement: A namespace's data drop removes its split parts
Dropping a namespace's data SHALL remove its split-list parts (prefix `0x04`) with its data (prefix `0x00`), in one
drop, as Dgraph's index, count and reverse drops pair each prefix with its split twin. Dgraph's `DeleteData` removes the
data prefix alone (ledger 7).

#### Scenario: Dropping a namespace's data leaves no split part
- **WHEN** a namespace whose lists are split has its data dropped
- **THEN** no key under `0x00` or `0x04` of the namespace remains, and its schema and type keys are unchanged

### Requirement: A request older than a drop or schema change it writes under fails
A request SHALL fail at commit with `Transaction has been aborted. Please retry`, and write nothing, when its read
timestamp is below the timestamp of a drop or a schema change and it writes a key under that drop or predicate. Dgraph
aborts its pending transactions on namespace and full drops, and refuses a predicate drop or schema update while
pending transactions touch the predicate.

#### Scenario: A request that read before a drop fails
- **WHEN** a request reads at 5, `name` is dropped at 8, and the request then commits a write to `name`
- **THEN** the commit fails with `Transaction has been aborted. Please retry`, and no key of `name` exists

### Requirement: A rollup does not write across a drop
A rollup that read a key before a drop covering it SHALL be discarded and write nothing. Dgraph's drops do not stop
rollups, so a rollup read before a drop can write the dropped data back (ledger 38).

#### Scenario: A rollup read before a drop writes nothing after it
- **WHEN** a rollup of a `name` list reads at 7, `name` is dropped at 8, and the rollup then writes
- **THEN** its batch is discarded, and no key of `name` exists
