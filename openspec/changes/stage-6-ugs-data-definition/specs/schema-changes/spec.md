## MODIFIED Requirements

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
default-entry records, with its catalog (`PROTOCOL.md` section 11.13). In a tenant that holds a schema, a Drop of a
predicate a schema declares SHALL be refused with code 7 (Refused), the message naming the predicate and stage 9.5, and
SHALL remove nothing. A Drop of a tenant's data or of everything in a tenant SHALL name the catalog version it drops
under, 0 for a tenant holding none, and SHALL be refused with code 2 (Conflict), kind 5 (Catalog), before any partition
sees it, when that is not the tenant's current version. A Drop SHALL remove a container whole, the history of
`@bitemporal` and `@append_only` types with it, and a Drop of a tenant's data or of everything SHALL be an audit event
naming the principal, the tenant and that version. A drop reaching several partitions is
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

#### Scenario: A declared predicate is not dropped in a governed tenant
- **WHEN** the aviation schema is in effect and a client sends a Drop of the predicate `av:serial`
- **THEN** the Drop is refused with code 7, the message naming `av:serial` and stage 9.5, and every `av:serial`
  statement is still read

#### Scenario: A Drop naming a stale catalog version is refused
- **WHEN** a tenant's catalog is at version 4 and a client sends a Drop of everything in it naming version 3
- **THEN** the Drop is refused with code 2, kind 5, no partition sees it, and every statement of the tenant is still read

#### Scenario: A Drop removes history with its tenant
- **WHEN** Gen5 is in effect, a `Company` is created and retracted, and everything in the tenant is dropped naming the
  current version
- **THEN** a read `with retracted` answers no row, and the audit log holds the Drop with its principal, tenant and version
