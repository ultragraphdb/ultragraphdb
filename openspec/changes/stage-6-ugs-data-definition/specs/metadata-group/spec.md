## MODIFIED Requirements

### Requirement: Catalog versions take effect at an epoch boundary
The metadata group SHALL hold each tenant's catalog as numbered versions whose contents it treats as opaque bytes. It
SHALL commit a new version as pending; at its next close `c + 1` the sequencer SHALL assign it the effective epoch
`c + 2` in the close's durable record and announce it with that close (sequencer, "Closes announce the catalog versions
taking effect after them"), and the group SHALL learn the epoch from that close, so every partition holds the version
before it can stamp its effective epoch. Every partition SHALL validate and apply a round against the version in effect
at the round's epoch. The group SHALL retain every version a partition's untrimmed log may name, and a partition SHALL
load those versions before it replays its log. A version whose predecessor is not the tenant's current version SHALL
be refused, naming both. Beside the opaque versions, the group SHALL hold default-entry records keyed by (tenant,
predicate), each the default entry a predicate's first write asks for (schema-storage, "Writes to an unknown predicate
create its schema"): recording one SHALL find a record that exists, SHALL create no catalog version, which the
predecessor check therefore ignores, and SHALL stale no prepared statement and abort no transaction. A default-entry
record SHALL outlive a drop of a predicate, a graph or a tenant's data, Describe naming it as recorded, and SHALL be
removed by a drop of everything in its tenant. A catalog version SHALL hold at most `catalog_bytes_max` bytes, 16,777,216
by default, carried in as many Catalog frames as they take (`PROTOCOL.md` sections 11.9 and 13.7): the frontend SHALL
refuse a larger one at its Commit, or at an Install compiling to one, with code 7, `detail` the bound, the message
naming its size, the catalog unchanged, and the group SHALL refuse to hold one.

#### Scenario: A version applies from its epoch
- **WHEN** catalog version 3 is committed as pending while epoch 38 is the latest close, the sequencer closes epoch 39,
  and rounds of epochs 39 and 40 commit
- **THEN** version 3 takes effect at epoch 40, assigned and announced with the close of 39, and the round of epoch 39 is
  validated against version 2 and the round of epoch 40 against version 3

#### Scenario: A default entry is no catalog version
- **WHEN** version 3 of `t1` is current, a first write of `score` records its default entry, and a client offers
  version 4 built on version 3
- **THEN** the default-entry record of (`t1`, `score`) is held, version 4 is accepted, and no prepared statement of
  `t1` is stale

#### Scenario: A stale version is refused
- **WHEN** a client offers version 4 built on version 2 while version 3 is current
- **THEN** the offer is refused with an error naming versions 2 and 3, and version 3 stays current

#### Scenario: A catalog version past its cap is refused at its Commit
- **WHEN** `catalog_bytes_max` is 523,000, a transaction's declarations build a catalog version of 523,001 bytes, and
  the client commits it
- **THEN** the Commit is refused with code 7, `detail` 523,000, the message naming the size, and the current version
  stays

#### Scenario: A catalog version past one frame travels in continuation frames
- **WHEN** a catalog version of 1,500,000 bytes is committed, and a storage process at the floor of `max_frame` reads it
  with Catalog
- **THEN** it arrives in several Catalog frames with one command header, the last alone flagged `LAST`, and their
  section 0 bytes concatenate to the version

#### Scenario: A default entry does not outlive a drop of everything
- **WHEN** tenant 3 holds a default-entry record, and everything in tenant 3 is dropped
- **THEN** the group holds no default-entry record of tenant 3

## ADDED Requirements

### Requirement: The metadata group holds compiled catalog versions with their sources
For a governed tenant the metadata group SHALL hold each catalog version as the canonical bytes of ugs-catalog,
opaque to it, with the sources the version was compiled from, committed as pending and effective at an epoch as every
catalog version is. Its owner SHALL compile an Install's sources with the tenant's, and SHALL commit a version only
when it builds on the tenant's current one. A version and its sources SHALL read back equal after a restart.

#### Scenario: A version survives a restart
- **WHEN** Gen5 is installed and the frontend is stopped and started
- **THEN** the metadata group answers the same version number, bytes and sources for the tenant

#### Scenario: Two installs on one base
- **WHEN** two Installs naming catalog version 0 of one empty tenant reach the owner
- **THEN** one commits version 1, and the other is refused as building on a version no longer current

### Requirement: The metadata group allocates type prefixes
The metadata group SHALL allocate type prefixes per tenant in its ordered log: each Install's new types take the next
free prefixes in the order the compiler names them, from 3, and the allocation SHALL be committed with the version it
belongs to, so an Install that is refused allocates nothing. A prefix SHALL never be allocated twice in a tenant, and
the next free prefix SHALL read back after a restart. Sequence blocks SHALL be leased per (type prefix, partition) for
declared types as for the built-in ones.

#### Scenario: A refused Install allocates nothing
- **WHEN** an Install declaring two new types is refused with code 28, and a later Install declares one new type
- **THEN** the later type takes the prefix the first of the refused two would have taken

#### Scenario: Allocation survives a restart
- **WHEN** the aviation schema is installed and the frontend is restarted
- **THEN** the metadata group answers the same prefix for each of the 13 types, and 16 as the tenant's next free prefix

#### Scenario: A declared type leases its own sequences
- **WHEN** a partition first creates an `Engine`
- **THEN** it leases a sequence block for prefix 5 and that partition, and the engine's sequence comes from it
