## MODIFIED Requirements

### Requirement: Adding an index rebuilds it from existing data
Declaring a tokenizer, an exact count index or a vector index on a predicate holding data SHALL build the new entries
from every existing object at the change's timestamp, in the background, while writes maintain the previous
declaration's indexes as well as the new one's. When the build succeeds, one atomic batch SHALL swap the indexes: for
every key under a rebuilt prefix, the built list, or a deletion where the build produced none, at the rebuild's
timestamp; for every key under a removed prefix, a deletion above every write that maintained it; and the new catalog
declaration. Until the rebuild completes, queries SHALL use an interim declaration: the new one without the indexes
being built and without the indexes being removed.

#### Scenario: A new index covers existing data
- **WHEN** a predicate holds 1,000 literals and an `exact` index is declared on it
- **THEN** after the rebuild, the index holds a token for each of the 1,000 literals

#### Scenario: Queries use the interim declaration during a rebuild
- **WHEN** a predicate indexed by `term` is declared with `exact` instead, and the `exact` index is being built
- **THEN** the declaration used for queries reports the predicate with neither `term` nor `exact`

#### Scenario: Writes during a rebuild are indexed
- **WHEN** a literal is written while its predicate's index is rebuilt
- **THEN** after the rebuild, the index holds the literal's tokens

#### Scenario: Writes during a rebuild leave no entry of a removed index
- **WHEN** `name` indexed by `term` is declared with `exact` instead, and `<b> <name> "pear"` is written while the
  `exact` index is built
- **THEN** after the rebuild, no term key of `name` remains, and the exact key for `pear` holds `b`

### Requirement: Removing an index drops only its entries
Removing a tokenizer, an exact count index or a vector index from a predicate's declaration SHALL drop that index's
keys, split parts included, and leave the others.

#### Scenario: Removing one tokenizer keeps another
- **WHEN** a predicate indexed by `term` and `exact` drops `term`
- **THEN** its term keys are gone and its exact keys remain

### Requirement: Type changes
A change to a predicate's declaration SHALL be refused, altering nothing, with an error naming the predicate and the
subject or object at fault, when the predicate's data contradicts it:
- declaring it single-valued while a subject holds more than one object of it;
- declaring it an edge while it holds a literal object, or a property while it holds an IRI or blank-node object;
- declaring a datatype constraint while it holds a literal of another datatype.

Declaring a predicate multi-valued, or of kind either, or removing its datatype constraint, SHALL always be applied. A
predicate holds an object when one of its forward lists holds it at the change's timestamp.

#### Scenario: A multi-valued predicate holding two objects cannot become single-valued
- **WHEN** `a` holds two objects of `nick` and `nick` is declared single-valued
- **THEN** the change fails with an error naming `nick` and `a`, and `nick` stays multi-valued

#### Scenario: A predicate holding literals cannot become an edge
- **WHEN** `name` holds `"Ann"` for `a` and `name` is declared an edge
- **THEN** the change fails with an error naming `name` and `a`

#### Scenario: A datatype constraint matching the data is applied
- **WHEN** `age` holds only `xsd:integer` literals and is declared with datatype `xsd:integer`
- **THEN** the change is applied

#### Scenario: Deltas summing to nothing hold no data
- **WHEN** `friend` gained `<a> <friend> <b>` at 3 and lost it at 4, and is declared a property at 5
- **THEN** the change is applied

### Requirement: Drops
Dropping a predicate SHALL remove, in every graph of its namespace, its forward, reverse, value-index, count-index,
vector-index and statistics keys, split parts included, the statement records of its statements, and the predicate
from the subject- and object-predicate lists that held it, and SHALL remove its catalog declaration. Dropping a graph
SHALL remove every key of the graph. Dropping a namespace's data SHALL keep its catalog. Dropping everything in a
namespace SHALL remove its data and catalog and write its initial catalog at timestamp 1. Dropping everything SHALL leave
no quad key but the root namespace's initial catalog, written at timestamp 1. Every drop SHALL remove what it drops
physically. A drop SHALL not remove dictionary entries.

#### Scenario: Dropping a predicate removes it from the predicate lists
- **WHEN** `knows` is held for `a` and `b` and is dropped
- **THEN** no key of `knows` remains, and neither the subject predicates of `a` nor of `b` holds `knows`

#### Scenario: Dropping a vector predicate drops its vector index
- **WHEN** `emb` is declared with a vector index, holds vectors, and is dropped
- **THEN** no forward or vector-index key of `emb` remains

#### Scenario: Dropping a graph keeps the other graphs
- **WHEN** graphs `<g1>` and `<g2>` hold statements and `<g1>` is dropped
- **THEN** no key of `<g1>` remains, and every statement of `<g2>` is still held

#### Scenario: Dropping data keeps the catalog
- **WHEN** a namespace's data is dropped
- **THEN** its predicates hold no data and its catalog is unchanged

#### Scenario: Dropping a namespace's everything survives a restart
- **WHEN** everything in namespace 3 is dropped and the store is reopened
- **THEN** namespace 3 holds only its initial catalog

#### Scenario: Dropping everything leaves the initial catalog
- **WHEN** everything is dropped
- **THEN** the store holds no quad key but the root namespace's initial catalog, written at timestamp 1

### Requirement: A namespace's data drop removes its split parts
Dropping a namespace's data SHALL remove every key of every quad keyspace of the namespace, split parts included, in one
drop, and leave its catalog keys.

#### Scenario: Dropping a namespace's data leaves no split part
- **WHEN** a namespace whose lists are split has its data dropped
- **THEN** no key of any quad keyspace of the namespace remains, split or not, and its catalog keys are unchanged

## REMOVED Requirements

### Requirement: A request older than a drop or schema change it writes under fails
**Reason**: The changeset applier reads and commits each batch under the lock its drops and index swaps take, so no
request reads before a drop or catalog change and commits after it.
**Migration**: None: a changeset applied while a drop runs waits for it and reads what it left.
