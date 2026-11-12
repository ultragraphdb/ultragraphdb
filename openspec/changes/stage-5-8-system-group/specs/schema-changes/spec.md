## ADDED Requirements

### Requirement: A restarted host of the system group takes a check up again
A tightening's check SHALL survive a restart of the system group's host: the version SHALL still be held as being
checked after recovery, the reports the metadata log holds SHALL count, the partitions SHALL send again each report
left unanswered, and the wait of `catalog_check_ms` SHALL begin anew. The Declare that asked for the version closed
with the host, which leaves the client's Commit in doubt as a lost reply does; Describe SHALL show what took effect
once the check ended.

#### Scenario: A check passes after the host restarts
- **WHEN** a declaration of `nick` single-valued is being checked over two partitions, one has reported its check
  passed, the system group's host is killed and restarted, and the other partition then reports its check passed
- **THEN** the version stands, Describe reports `nick` single-valued, and the client's Commit, whose reply was lost, is
  shown committed by Describe

## MODIFIED Requirements

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
this stage"). The frontend SHALL ask the system group, by Declare of kind 2, for the catalog version a Drop calls for,
committed there as pending, before it relays the Drop's reply, stored or new, and the system group SHALL build no
version that would change nothing, so a frontend that stopped in between leaves the step to the client's resend; a Drop
through no session has no resend, and a client that lost its reply runs the Drop again, which removes nothing more and
builds that version.

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
- **WHEN** a Drop of `p` through a session commits at its partition, the frontend stops before the catalog
  version removing `p`'s declaration is asked for, and the client resends the Drop after the frontend restarts
- **THEN** the partition's table answers the stored reply, the system group commits that version as pending, asked by the frontend before it relays the reply, and `p` is dropped once

#### Scenario: A Drop through no session runs again
- **WHEN** a Drop of `p` through no session commits at its partition, its reply is lost, and the client sends the Drop
  again
- **THEN** the second Drop removes nothing more, and the system group builds the version removing `p`'s declaration, asked by the frontend before it relays the reply

#### Scenario: A default entry outlives a drop
- **WHEN** `score` holds a default-entry record, and a Drop of `score` commits
- **THEN** the metadata group still holds the record, and Describe names `score` as recorded

### Requirement: A tightening is checked before it is acknowledged
A declaring Commit that tightens a predicate SHALL be answered only once every partition holding the tenant at the
catalog version's effective epoch has reported its check passed, and the system group SHALL wait `catalog_check_ms` for
the reports, durable in the metadata group, holding the frontend's Declare open until the check ends. A partition's
refusal SHALL refuse the Commit with code 7 naming the predicate and the subject or object at fault; a partition silent
past the wait SHALL refuse it with code 24, `detail` that partition, naming the partition and the version. Each SHALL
withdraw the version in one command with the offer of the version restoring the previous declaration, durable before the
refusal. Describe SHALL report no version being checked or withdrawn. A version built on one being checked SHALL be
refused with code 2, kind 5. Declarations tightening one predicate or aspect and loosening another SHALL be refused at
the Commit with code 7, `detail` the index of the first that loosens, the catalog unchanged. The check SHALL be the
system group's: a frontend that stops changes nothing about it, and a host of the group that restarts SHALL take the
check up again and wait `catalog_check_ms` anew.

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

#### Scenario: A check outlives the frontend that asked for it
- **WHEN** a client commits a declaration of `nick` single-valued, the frontend stops before any partition reports, and a partition then reports that the data refuses it
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
