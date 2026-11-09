## RENAMED Requirements

- FROM: `### Requirement: A namespace's data drop removes its split parts`
- TO: `### Requirement: A tenant's data drop removes its split parts`

## MODIFIED Requirements

### Requirement: Adding an index rebuilds it from existing data
Declaring a tokenizer, an exact count index or a vector index on a predicate holding data SHALL build the new entries in
every partition holding the tenant, from every existing object at the build's start version, the first of the
catalog version's effective epoch, in the background, while writes from that epoch maintain the previous declaration's
indexes as well as the new one's (commit-rounds, "Catalog changes, drops and index builds are ordered in rounds").
When a partition's build succeeds, one atomic batch, logged as the build's completion record, SHALL swap its indexes:
for every key under a rebuilt prefix, the built list, or a deletion where the build produced none, at the build's
version; for every key under a removed prefix, a deletion above every write that maintained it. The batch SHALL hold
index keys only, since no partition holds catalog keys. Until a partition's build completes, queries reading that
partition SHALL use an interim declaration: the new one without the indexes being built and without the indexes being
removed.

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

#### Scenario: Each partition swaps on its own
- **WHEN** an `exact` index is declared over two partitions, and partition 1's build completes before partition 2's
- **THEN** queries reading partition 1 use the new declaration, queries reading partition 2 the interim one until its
  completion record is durable, and neither swap writes a catalog key

### Requirement: Type changes
A change to a predicate's declaration SHALL be checked against the predicate's data by every partition holding the
tenant at the catalog version's effective epoch, each holding its commits of that epoch until its check completes,
and the change's reply SHALL wait for every check. When the data contradicts it in any partition, the change SHALL be
refused with an error naming the predicate and the subject or object at fault, and the metadata group SHALL commit a
version restoring the previous declaration, the partitions validating the predicate's writes against the previous
declaration until that version takes effect, so the refused change alters no data. The data contradicts a change:
- declaring it single-valued while a subject holds more than one object of it;
- declaring it an edge while it holds a literal object, or a property while it holds an IRI or blank-node object;
- declaring a datatype constraint while it holds a literal of another datatype.

Declaring a predicate multi-valued, or of kind either, or removing its datatype constraint, SHALL always be applied. A
predicate holds an object when one of its forward lists holds it at the start of the version's effective epoch.

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

#### Scenario: A contradiction written before the change takes effect refuses it
- **WHEN** `nick` is declared single-valued while `a` holds one object of it, and a commit adding a second object of
  `nick` to `a` lands in the epoch before the version takes effect
- **THEN** the check at the effective epoch fails naming `nick` and `a`, the change's reply is that error, and `nick`
  stays multi-valued

### Requirement: A failed rebuild restores the previous schema and its indexes
A rebuild that fails in a partition SHALL leave every index the previous declaration names complete in that partition
and report the failure to the metadata group, which SHALL commit a version restoring the previous declaration; until
that version takes effect, every partition SHALL keep maintaining the previous declaration's indexes and queries SHALL
use the interim declaration.

#### Scenario: A failed rebuild keeps the old index usable
- **WHEN** `term` is replaced by `exact` and the `exact` build fails
- **THEN** once the restoring version takes effect the schema declares `term`, and every term key is present

### Requirement: Drops
Dropping a predicate SHALL remove, in every graph of its tenant and in every partition holding the tenant, its forward,
reverse, value-index, count-index, vector-index and statistics keys, split parts included, the statement records of its
statements, and the predicate from the subject- and object-predicate lists that held it, and SHALL remove its
declaration from the metadata group. Dropping a graph SHALL remove every key of the graph in the tenant. Dropping a
tenant's data SHALL keep its catalog. Dropping everything in a tenant SHALL remove its data and set its catalog in the
metadata group to the initial catalog, which removes every namespace installed in it. A drop's keys all hold its tenant,
so another tenant sharing the bucket run SHALL keep its own, and no drop SHALL span tenants: a cluster wipe is the
removal of each tenant (metadata-group, "The metadata group holds the tenants"). No partition SHALL hold catalog keys.
Every drop SHALL remove what it drops physically. A drop SHALL not remove dictionary entries or default-entry records. A
drop reaching several partitions is cross-partition (partitions, "A read-write transaction stays within one partition in
this stage"). The frontend SHALL commit the catalog version a Drop calls for as pending before it relays the Drop's
reply, stored or new, and SHALL build no version that would change nothing, so a frontend that stopped in between leaves
the step to the client's resend; a Drop through no session has no resend, and a client that lost its reply runs the Drop
again, which removes nothing more and builds that version.

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
- **WHEN** a tenant's data is dropped
- **THEN** its predicates hold no data and its catalog is unchanged

#### Scenario: Dropping a tenant's everything survives a restart
- **WHEN** everything in tenant 3 is dropped and the roles are restarted
- **THEN** tenant 3 holds no quad key, and the metadata group holds its initial catalog

#### Scenario: A tenant's drop leaves a tenant sharing its run
- **WHEN** tenants 7 and 8 share a bucket run, each holds statements in one partition, and everything in tenant 7 is
  dropped
- **THEN** tenant 7 holds no quad key, its catalog is the initial catalog, and every statement and the catalog of tenant
  8 are unchanged

#### Scenario: A resent Drop builds the version a stopped frontend left
- **WHEN** a Drop of `p` through a session commits at its partition, the frontend stops before it commits the catalog
  version removing `p`'s declaration, and the client resends the Drop after the frontend restarts
- **THEN** the partition's table answers the stored reply, the frontend commits that version as pending before it relays
  the reply, and `p` is dropped once

#### Scenario: A Drop through no session runs again
- **WHEN** a Drop of `p` through no session commits at its partition, its reply is lost, and the client sends the Drop
  again
- **THEN** the second Drop removes nothing more, and the frontend builds the version removing `p`'s declaration before
  it relays the reply

#### Scenario: A default entry outlives a drop
- **WHEN** `score` holds a default-entry record, and a Drop of `score` commits
- **THEN** the metadata group still holds the record, and Describe names `score` as recorded

### Requirement: A tenant's data drop removes its split parts
Dropping a tenant's data SHALL remove every key of every quad keyspace of the tenant, split parts included, in one
drop, and leave its catalog in the metadata group.

#### Scenario: Dropping a tenant's data leaves no split part
- **WHEN** a tenant whose lists are split has its data dropped
- **THEN** no key of any quad keyspace of the tenant remains, split or not, and its catalog is unchanged

## ADDED Requirements

### Requirement: A tightening is checked before it is acknowledged
A declaring Commit that tightens a predicate SHALL be answered only once every partition holding the tenant at the catalog
version's effective epoch has reported its check passed, and the frontend SHALL wait `catalog_check_ms` for the reports, durable
in the metadata group. A partition's refusal SHALL refuse the Commit with code 7 naming the predicate and the subject or object at
fault; a partition silent past the wait SHALL refuse it with code 24, `detail` that partition, naming the partition and the version.
Each SHALL withdraw the version in one command with the offer of the version restoring the previous declaration, durable before the
refusal. Describe SHALL report no version being checked or withdrawn. A version built on one being checked SHALL be refused with code
2, kind 5. Declarations tightening one predicate or aspect and loosening another SHALL be refused at the Commit with code 7, `detail`
the index of the first that loosens, the catalog unchanged. A frontend that stops SHALL take the check up again from the metadata
group.

#### Scenario: A tightening the data contradicts is refused and restored
- **WHEN** `a` holds two objects of `nick` and a client commits a declaration of `nick` single-valued
- **THEN** the Commit is refused with code 7 naming `nick` and `a`, the metadata group holds a version restoring the previous
  declaration, and no Describe reports `nick` single-valued

#### Scenario: A tightening the data bears out is acknowledged after the check
- **WHEN** a client commits a declaration of `nick` single-valued over two partitions holding `a` with one object of it, and one has
  reported its check passed
- **THEN** the Commit stays unanswered and Describe reports `nick` as before until the other reports, and then the Commit answers the
  first version of the effective epoch and Describe reports `nick` single-valued

#### Scenario: A partition silent past the wait withdraws the tightening
- **WHEN** a client commits a declaration of `nick` single-valued, and a partition holding the tenant reports nothing within
  `catalog_check_ms`
- **THEN** the Commit is refused with code 24, `detail` that partition, naming it and the version, the metadata group holds a version
  restoring the previous declaration, and a report arriving later changes nothing

#### Scenario: A version built on one being checked is refused
- **WHEN** a client commits a declaration of `nick` single-valued, and before any partition reports a second transaction begun on that
  version commits a declaration of `age`
- **THEN** the second Commit is refused with code 2, kind 5, naming the version being checked, and once the first is answered a third
  transaction's declaration of `age` commits

#### Scenario: Declarations tightening and loosening at once are refused at the Commit
- **WHEN** `friend` is declared an edge, and a client commits a declaration of `friend` a property
- **THEN** the Commit is refused with code 7, `detail` its declaration's index, naming `friend`, and the catalog is unchanged

#### Scenario: A restarted frontend finishes the check it held
- **WHEN** a client commits a declaration of `nick` single-valued, the frontend stops before any partition reports, and after its
  restart a partition reports that the data refuses it
- **THEN** the metadata group holds a version restoring the previous declaration, and no Describe reports `nick` single-valued

#### Scenario: A Drop during a check stands against the restoring version
- **WHEN** a client commits a declaration of `nick` and of `age` single-valued, a Drop of `nick` builds its version before any partition
  reports, and a partition reports that the data refuses the first
- **THEN** the latest version holds neither `nick` nor `age` single-valued, and `nick` declared by no version after the Drop's

#### Scenario: A partition holding no term of a predicate writes it under its declaration
- **WHEN** a client's declaration of a new `name` a property is acknowledged over two partitions, and a client writes an IRI object of
  `name` to a subject of each
- **THEN** each write is refused

#### Scenario: A write the tightening forbids is refused once the check passed
- **WHEN** a client's declaration of `name` a property is acknowledged, `a` holding a literal of it, the storage process restarts, and
  a client writes `<a> <name> <b>`
- **THEN** the write is refused, and `a` holds its literal alone

#### Scenario: A tightening passes every partition's check through the binaries
- **WHEN** `a` and `b`, one in each of two partitions, each hold one object of `nick`, and a client commits a declaration of `nick`
  single-valued
- **THEN** both partitions have reported when the Commit answers the first version of the effective epoch, and Describe reports `nick`
  single-valued

### Requirement: A schema change takes effect at an epoch boundary
A schema change SHALL take effect in every partition from the epoch the sequencer assigns its catalog version: a
version the metadata group holds pending at close `c + 1` takes effect at `c + 2` (metadata-group, "Catalog versions
take effect at an epoch boundary"); a round of an earlier epoch SHALL be validated against the previous version, and
an index rebuild the change starts SHALL begin in each partition at that epoch. The version SHALL reach every partition
with the close of `c + 1`, and a partition stamps epoch `c + 2` only once that close is delivered, so no partition
commits a round of the assigned epoch before it holds the new version.

#### Scenario: Two partitions switch at the same epoch
- **WHEN** an index is added while epoch 48 is the latest close, over two partitions, and the sequencer closes epoch 49
- **THEN** the change takes effect at epoch 50, and in both partitions rounds of epoch 49 maintain no entries of the
  index and rounds of epoch 50 do

#### Scenario: A commit behind a version's round is validated against it
- **WHEN** a transaction executed against catalog version 2 commits behind the round taking version 3 effect, before the close
  announcing version 3 is delivered
- **THEN** it aborts with a conflict of kind Catalog, and a transaction executed against version 3 commits

#### Scenario: A commit-time operation against an earlier version aborts
- **WHEN** a transaction with a commit-time operation, executed against catalog version 2, commits once version 3 is in effect
- **THEN** it aborts with a conflict of kind Catalog, and its operation runs not at all

#### Scenario: A partition holds the version before it stamps its epoch
- **WHEN** an index is added while epoch 48 is the latest close, and a partition's latest delivered close is 48
- **THEN** the partition's commits carry epoch 49 until the close of 49, carrying the new version, is delivered to it,
  and its first commit at epoch 50 is validated against the new version
