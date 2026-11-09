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
Every drop SHALL remove what it drops physically. A drop SHALL not remove dictionary entries, and only a drop of
everything in a tenant SHALL remove the tenant's default-entry records, with its catalog. In a tenant that holds a
schema, a Drop of a predicate a schema declares SHALL be refused with code 7 (Refused), the message naming the
predicate and stage 9.5, and SHALL remove nothing. A
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

#### Scenario: Dropping everything removes the default entries
- **WHEN** tenant 3 holds a default-entry record of `score`, and everything in tenant 3 is dropped
- **THEN** the metadata group holds no default-entry record of tenant 3, and its catalog is the initial catalog

#### Scenario: A declared predicate is not dropped in a governed tenant
- **WHEN** the aviation schema is in effect and a client sends a Drop of the predicate `av:serial`
- **THEN** the Drop is refused with code 7, the message naming `av:serial` and stage 9.5, and every `av:serial`
  statement is still read
