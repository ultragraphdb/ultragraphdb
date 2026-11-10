## ADDED Requirements

### Requirement: Drops, purges and builds are logged operations every replica applies alike
Every drop SHALL be the last record of its round, operations 1 to 4 of the write-set record, and so SHALL every purge,
one operation 9 per instance it removes in the partition; a build's completion, operation 7 with its outcome, a swap or
an undo, SHALL be a record of entry kind 3, at most one in a round (`specification/binary_protocol/PROTOCOL.md` section
12.2). Every replica and replay SHALL apply each at its logged version and position, from the record alone: a drop as a
drop marker and a purge as a purge marker per instance (`specification/storage/STORAGE.md` section 15.5), reads at
every version answering nothing they cover and compaction removing it, class I entries included, at boundaries the log
decides (tile-engine, "Drops and purges are logged markers that reads honour and compaction removes"); a build by beats
the log decides, resuming from its build record (tile-engine, "Index builds run beside compaction and resume from their
record"). A purge SHALL write its marker at the instance's home and, through the purge's share, at each member of its
referrer set, and a purge at version `v` SHALL abort every open read-write transaction of its partition whose read
version is below `v`.

#### Scenario: Two replicas apply a drop alike
- **WHEN** two replicas of one partition apply the same rounds, one of them holding a drop of `p`
- **THEN** from that round on neither answers an entry of `p` at any version, and their checkpoints after it have one
  identity

#### Scenario: Replay of a purge hides the instance
- **WHEN** a round purging instance `i` is applied, the store is discarded, and the log is replayed into an empty
  store
- **THEN** no read at any version answers an entry naming `i`, and no tile compaction writes holds one

## MODIFIED Requirements

### Requirement: Adding an index rebuilds it from existing data
Declaring a tokenizer or an exact count index on a predicate holding data SHALL build the new entries in every
partition holding the tenant, from every existing object at the build's start version, the first of the catalog
version's effective epoch, while writes from that epoch maintain the previous declaration's indexes as well as the new
one's (commit-rounds, "Catalog changes, drops and index builds are ordered in rounds"). The build SHALL walk its source
in paced beats beside compaction, writing the index family as compaction outputs created at its start version, which
the fold horizon stays at or below while it runs, and its build record (`STORAGE.md` section 9.3, kind 15) SHALL hold
the last key reached, updated at each checkpoint, from which a recovering replica resumes strictly after it (tile-engine,
"Index builds run beside compaction and resume from their record"). A tenant holding a schema SHALL build no index
before stage 9.5 (`STORAGE.md` section 16.7). When a partition's build succeeds, its completion, logged as a swap,
SHALL make the built index family live at the build's version and remove the index families of the tokenizers the
version does not declare (`PROTOCOL.md` section 12.2, operation 7), and SHALL write no catalog key, since no partition
holds catalog keys. Until a partition's build completes, queries reading that partition SHALL use an interim
declaration: the new one without the indexes being built and without the indexes being removed.

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
- **THEN** after the rebuild, no read answers a `term` index entry of `name`, and the `exact` index lists `b` under
  `pear`

#### Scenario: Each partition swaps on its own
- **WHEN** an `exact` index is declared over two partitions, and partition 1's build completes before partition 2's
- **THEN** queries reading partition 1 use the new declaration, queries reading partition 2 the interim one until its
  completion record is durable, and neither swap writes a catalog key

#### Scenario: A build interrupted by a crash resumes where its record stands
- **WHEN** a partition building an `exact` index is killed after a checkpoint whose build record names the last key its
  backfill reached
- **THEN** recovery resumes the backfill strictly after that key, and after the swap the index equals that of a
  partition that was not killed

### Requirement: Removing an index drops only its entries
Removing a tokenizer or an exact count index from a predicate's declaration SHALL remove that index's entries in every
partition holding the tenant, and leave the others. When the catalog version taking effect (`PROTOCOL.md` section 12.2,
operation 6) removes a tokenizer of an index and starts no build of that index whose swap removes it, its
round SHALL write a drop marker naming operation 6 and the version, covering that tokenizer's index families, which
are removed as a drop removes families (`STORAGE.md` section 15.5; tile-engine, "Drops and purges are logged markers
that reads honour and compaction removes"; T48).

#### Scenario: Removing one tokenizer keeps another
- **WHEN** a predicate indexed by `term` and `exact` drops `term`
- **THEN** no read answers a `term` index entry of the predicate, and its `exact` index answers as before

#### Scenario: A tokenizer removed without a build leaves under a drop marker
- **WHEN** a catalog version declaring `name` with `exact` alone, where it held `term` and `exact`, takes effect
- **THEN** that version's round writes a drop marker covering `name`'s `term` index families, no read answers a `term`
  entry of `name` from that round on, and the families' tiles are due at its boundary

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
predicate holds an object when a statement of it with that object is visible at the first version of the version's
effective epoch.

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
A rebuild that fails in a partition SHALL leave every index the previous declaration names complete in that partition,
its completion logged as an undo that removes the built family and restores the previous declaration, which replay
undoes again (`PROTOCOL.md` section 12.2, operation 7), and report the failure to the metadata group, which SHALL
commit a version restoring the previous declaration; until that version takes effect, every partition SHALL keep
maintaining the previous declaration's indexes and queries SHALL use the interim declaration.

#### Scenario: A failed rebuild keeps the old index usable
- **WHEN** `term` is replaced by `exact` and the `exact` build fails
- **THEN** once the restoring version takes effect the schema declares `term`, and the `term` index lists every
  literal's tokens

### Requirement: Drops
Dropping a predicate SHALL remove, in every graph of its tenant and in every partition holding the tenant, its forward,
reverse, index and count entries, its statistics and the statement records of its statements, and SHALL remove its
declaration from the metadata group. Dropping a graph SHALL remove every entry of the graph in the tenant, its families
leaving the directory at the drop's round. Dropping a tenant's data SHALL keep its catalog. Dropping everything in a
tenant SHALL remove its data and set its catalog in the metadata group to the initial catalog, which removes every
namespace installed in it. A drop's entries all hold its tenant, so another tenant sharing the bucket run SHALL keep its
own, and no drop SHALL span tenants: a cluster wipe is the removal of each tenant (metadata-group, "The metadata group
holds the tenants"). No partition SHALL hold catalog keys. A drop SHALL take effect at its round as a drop marker, under
which reads at every version answer nothing it covers, and every drop SHALL remove what it drops physically, compaction
writing none of it back (tile-engine, "Drops and purges are logged markers that reads honour and compaction removes"). A
drop SHALL not remove dictionary entries, and only a drop of everything in a tenant SHALL remove the tenant's
default-entry records, with its catalog (`PROTOCOL.md` section 11.13). A drop reaching several partitions is
cross-partition (partitions, "A read-write transaction stays within one partition in this stage"). The frontend SHALL
commit the catalog version a Drop calls for as pending before it relays the Drop's reply, stored or new, and SHALL build
no version that would change nothing, so a frontend that stopped in between leaves the step to the client's resend; a
Drop through no session has no resend, and a client that lost its reply runs the Drop again, which removes nothing more
and builds that version.

#### Scenario: Dropping a predicate leaves none of its entries
- **WHEN** `knows` is held for `a` and `b` and is dropped
- **THEN** no read answers an entry of `knows`, and a read of every predicate of `a` or of `b` names no `knows`

#### Scenario: Dropping a predicate holding vectors drops its values
- **WHEN** `emb` holds vector values and is dropped
- **THEN** no read answers a value of `emb`

#### Scenario: Dropping a graph keeps the other graphs
- **WHEN** graphs `<g1>` and `<g2>` hold statements and `<g1>` is dropped
- **THEN** no read answers an entry of `<g1>`, and every statement of `<g2>` is still held

#### Scenario: Dropping data keeps the catalog
- **WHEN** a tenant's data is dropped
- **THEN** its predicates hold no data and its catalog is unchanged

#### Scenario: Dropping a tenant's everything survives a restart
- **WHEN** everything in tenant 3 is dropped and the roles are restarted
- **THEN** no read answers a statement of tenant 3, and the metadata group holds its initial catalog

#### Scenario: A tenant's drop leaves a tenant sharing its run
- **WHEN** tenants 7 and 8 share a bucket run, each holds statements in one partition, and everything in tenant 7 is
  dropped
- **THEN** no read answers a statement of tenant 7, its catalog is the initial catalog, and every statement and the
  catalog of tenant 8 are unchanged

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

#### Scenario: Dropping everything removes the default entries
- **WHEN** tenant 3 holds a default-entry record of `score`, and everything in tenant 3 is dropped
- **THEN** the metadata group holds no default-entry record of tenant 3, and its catalog is the initial catalog

## REMOVED Requirements

### Requirement: A rollup does not write across a drop
**Reason**: The tile store runs no rollup. A drop is a drop marker under which every read answers nothing it covers, and
compaction removes what it covers, deciding at boundaries the log fixes, so no rewrite writes dropped entries back
(`specification/storage/STORAGE.md` sections 15.5 and 16.3).
**Migration**: "Drops" and "Drops, purges and builds are logged operations every replica applies alike" state the rule;
tile-engine's "Drops and purges are logged markers that reads honour and compaction removes" states the mechanism.

### Requirement: A tenant's data drop removes its split parts
**Reason**: The tile store has no quad keyspaces and no split lists: a long row is cut into tiles by compaction, and a
drop of a tenant's data covers every family of the tenant whatever its tiles (`specification/storage/STORAGE.md`
sections 15.5 and 16.3).
**Migration**: "Drops" states what a drop of a tenant's data removes and that it keeps the catalog.
