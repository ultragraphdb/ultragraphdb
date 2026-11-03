## ADDED Requirements

### Requirement: The metadata group is a log-backed state machine with one replica
The metadata group's state SHALL be a function of its log: every command SHALL be appended and applied in log order,
acknowledged only once durable, and recovery SHALL rebuild the state by replaying the log from the last snapshot. In
this stage it runs with one replica inside the frontend; its interface SHALL be the one stage 8 replicates. Reads of its
state SHALL name the log position they read at.

#### Scenario: Recovery rebuilds the state
- **WHEN** the group applies a catalog version, two block leases, a placement change and a principal, and is killed and
  restarted
- **THEN** its state after recovery equals its state before the kill, read at the same log position

#### Scenario: A snapshot shortens recovery
- **WHEN** a snapshot is taken at position 100 and 20 more commands are applied before a restart
- **THEN** recovery loads the snapshot and replays exactly the 20 commands after it

### Requirement: Catalog versions take effect at an epoch boundary
The metadata group SHALL hold each namespace's catalog as numbered versions whose contents it treats as opaque bytes. It
SHALL commit a new version as pending; at its next close `c + 1` the sequencer SHALL assign it the effective epoch
`c + 2` in the close's durable record and announce it with that close (sequencer, "Closes announce the catalog versions
taking effect after them"), and the group SHALL learn the epoch from that close, so every partition holds the version
before it can stamp its effective epoch. Every partition SHALL validate and apply a round against the version in effect
at the round's epoch. The group SHALL retain every version a partition's untrimmed log may name, and a partition SHALL
load those versions before it replays its log. A version whose predecessor is not the namespace's current version SHALL
be refused, naming both. Beside the opaque versions, the group SHALL hold default-entry records keyed by (namespace,
predicate), each the default entry a predicate's first write asks for (schema-storage, "Writes to an unknown predicate
create its schema"): recording one SHALL find a record that exists, SHALL create no catalog version, which the
predecessor check therefore ignores, and SHALL stale no prepared statement and abort no transaction.

#### Scenario: A version applies from its epoch
- **WHEN** catalog version 3 is committed as pending while epoch 38 is the latest close, the sequencer closes epoch 39,
  and rounds of epochs 39 and 40 commit
- **THEN** version 3 takes effect at epoch 40, assigned and announced with the close of 39, and the round of epoch 39 is
  validated against version 2 and the round of epoch 40 against version 3

#### Scenario: A default entry is no catalog version
- **WHEN** version 3 of `ns` is current, a first write of `score` records its default entry, and a client offers
  version 4 built on version 3
- **THEN** the default-entry record of (`ns`, `score`) is held, version 4 is accepted, and no prepared statement of
  `ns` is stale

#### Scenario: A stale version is refused
- **WHEN** a client offers version 4 built on version 2 while version 3 is current
- **THEN** the offer is refused with an error naming versions 2 and 3, and version 3 stays current

### Requirement: The metadata group leases sequence blocks
The metadata group SHALL lease sequence blocks per (type, partition) from a per-type high-water mark that only rises,
each block a half-open range of the size the partition asks for, at most `block_size_max`. A lease SHALL be durable
before it is answered, so no block is leased twice across a crash.

#### Scenario: Blocks never overlap
- **WHEN** partitions 1 and 2 lease blocks of the `Resource` type alternately ten times
- **THEN** every block begins at the previous block's end, and no two overlap

#### Scenario: A lease survives a crash
- **WHEN** a block 1..1001 is leased and the group is killed and restarted
- **THEN** the next lease of the same type begins at 1001

#### Scenario: An oversized request is refused
- **WHEN** a partition asks for a block larger than `block_size_max`
- **THEN** the lease is refused with an error naming the size and the maximum

### Requirement: The placement map assigns bucket ranges to partitions
The metadata group SHALL hold the placement map: every bucket assigned to exactly one partition as half-open ranges of
bucket numbers, each namespace's run of 65,536 buckets (the buckets whose top 8 bits are its hash) and its 2^10 bucket
groups of 64 buckets, and each partition's preferred leader. A change to the map SHALL keep every bucket assigned
exactly once and SHALL be refused otherwise, naming the first bucket left unassigned or assigned twice. A change SHALL
name no epoch: the group SHALL commit it as a pending placement version, numbered one above the last (the initial map is
version 0, in effect from epoch 0); at its next close `c + 1` the sequencer SHALL assign the oldest pending placement
version, and at most that one, the effective epoch `c + 2` in the close's durable record and announce it with that close
(sequencer, "Closes announce the versions taking effect after them"), and the group SHALL learn the epoch from that
close, refusing a close that announces any placement version but the oldest pending. A bucket, and a bucket group,
SHALL resolve to the partitions the version in effect at an epoch gives it, at any epoch a read names; no change
rewrites the placement of an epoch a partition may have opened. In this stage the map starts with all buckets of the
namespaces that exist split evenly over the configured partitions.

#### Scenario: The initial map covers every bucket once
- **WHEN** the map is initialised for one namespace and three partitions
- **THEN** that namespace's 65,536 buckets are assigned in three contiguous ranges whose sizes differ by at most one,
  and every other bucket is assigned to a partition

#### Scenario: A change leaving a bucket unassigned is refused
- **WHEN** a change moves the range 100..200 from partition 1 to nowhere
- **THEN** the change is refused with an error naming bucket 100, and the map is unchanged

#### Scenario: A bucket's partition is found
- **WHEN** partition 2 holds the range 21,846..43,691 and bucket 30,000 is looked up
- **THEN** the lookup answers partition 2

#### Scenario: A bucket group resolves at an epoch
- **WHEN** a change moving buckets 21,888..21,952 from partition 2 to partition 3 applies from epoch 21, and the bucket
  group holding those 64 buckets is resolved at epochs 20 and 21
- **THEN** it resolves to partition 2 at epoch 20 and to partition 3 at epoch 21

#### Scenario: A placement change takes effect two epochs after the latest close
- **WHEN** a change moving buckets 21,888..21,952 from partition 2 to partition 3 is committed while epoch 19 is the
  latest close, and the sequencer closes epoch 20
- **THEN** the close of epoch 20 assigns it effective epoch 21, and the bucket group holding those 64 buckets resolves to
  partition 2 at epoch 20 and to partition 3 at epoch 21

#### Scenario: A close announcing a placement version out of order is refused
- **WHEN** placement versions 1 and 2 are pending and a close announces version 2
- **THEN** the group refuses the close, and both versions stay pending

### Requirement: Principals live in the metadata group
The metadata group SHALL hold the cluster's principals: each a name and its SCRAM-SHA-256 credential (salt, iteration
count, stored key and server key), never the password. The first principal SHALL be created at the cluster's first start
from a credential the operator supplies through a secret file, and a cluster with no principal SHALL refuse every
connection. Creating a principal whose name exists SHALL be refused, naming it; removing the last principal with the
administrator role SHALL be refused.

#### Scenario: The bootstrap principal authenticates
- **WHEN** a cluster starts for the first time with a bootstrap credential file and a client authenticates with it
- **THEN** authentication succeeds, and the metadata group holds that principal without the password

#### Scenario: A duplicate principal is refused
- **WHEN** a principal named `ann` exists and another `ann` is created
- **THEN** the creation is refused with an error naming `ann`

#### Scenario: The last administrator stays
- **WHEN** the only principal with the administrator role is removed
- **THEN** the removal is refused with an error naming it

### Requirement: The metadata group registers sessions
The metadata group SHALL record each session the frontend registers, its 128-bit identity and its principal, as one
command, and SHALL answer a session's principal to the frontend's owner check. A registration SHALL be durable before it
is answered. The registry SHALL hold at most `sessions_max` sessions, evicting the least recently registered, and SHALL
be the only place a session is evicted; the sequencer SHALL carry each eviction to every partition with its next close,
recorded in its durable close record, so each partition purges the session's entry from its table. A session already
registered SHALL be refused, naming it.

#### Scenario: A registered session survives a restart
- **WHEN** a session is registered for `alice` and the group is killed and restarted
- **THEN** the session's principal is still `alice`

#### Scenario: A full registry evicts the oldest session
- **WHEN** the registry holds `sessions_max` sessions and one more is registered
- **THEN** the least recently registered session is evicted, and a commit through it is refused as evicted
