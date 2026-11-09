# metadata-group Specification

## Purpose
The metadata group: a log-backed state machine with one replica inside the frontend, holding what the cluster agrees
on: the tenants, each tenant's catalog versions taking effect at an epoch boundary, sequence-block leases, the placement
map of bucket ranges to partitions, the principals and the session registry, so its state recovers by replaying its log
behind the interface that replication builds on.

## Requirements
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
record SHALL outlive a drop, and Describe names it as recorded. A catalog version SHALL hold at most 523,000 bytes, what one
Catalog frame holds at the floor of `max_frame`: the frontend SHALL refuse a larger one at its Commit, or at an Install compiling
to one, with code 7, `detail` 523,000, the message naming its size, the catalog unchanged, and the group SHALL refuse to hold one.

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
- **WHEN** a transaction's declarations build a catalog version of 523,001 bytes, and the client commits it
- **THEN** the Commit is refused with code 7, `detail` 523,000, the message naming the size, and the current version
  stays

#### Scenario: A catalog version at its cap travels in one Catalog frame
- **WHEN** a catalog version of 523,000 bytes is committed, and a storage process at the floor of `max_frame` reads it
  with Catalog
- **THEN** one Catalog frame holds its whole bytes

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
bucket numbers, each tenant's run of 65,536 buckets (the buckets whose top 8 bits are its hash), which tenants whose hashes
agree there share, since at most 256 runs exist and isolation is by key and by grant, not by resource, and each run's 2^10
bucket groups of 64 buckets, and each partition's preferred leader. A change to the map SHALL keep every bucket assigned
exactly once and SHALL be refused otherwise, naming the first bucket left unassigned or assigned twice. A change SHALL
name no epoch: the group SHALL commit it as a pending placement version, numbered one above the last (the initial map is
version 0, in effect from epoch 0); at its next close `c + 1` the sequencer SHALL assign the oldest pending placement
version, and at most that one, the effective epoch `c + 2` in the close's durable record and announce it with that close
(sequencer, "Closes announce the versions taking effect after them"), and the group SHALL learn the epoch from that
close, refusing a close that announces any placement version but the oldest pending. A bucket, and a bucket group,
SHALL resolve to the partitions the version in effect at an epoch gives it, at any epoch a read names; no change
rewrites the placement of an epoch a partition may have opened. In this stage the map starts with every run split evenly
over the configured partitions, so a tenant created later spreads as tenant 0 does.

#### Scenario: The initial map covers every bucket once
- **WHEN** the map is initialised for one tenant and three partitions
- **THEN** that tenant's 65,536 buckets are assigned in three contiguous ranges whose sizes differ by at most one,
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

#### Scenario: Two tenants share a run
- **WHEN** the metadata group holds tenants 7 and 8, whose hashes share their top 8 bits, and a role reads Placement
- **THEN** Placement's runs list each tenant once, both with the one `run`, and the bucket ranges are unchanged by
  either

### Requirement: Principals live in the metadata group
The metadata group SHALL hold the cluster's principals: each a name and its SCRAM-SHA-256 credential (salt, iteration
count, stored key and server key), never the password. The first principal SHALL be created at the cluster's first start
from a credential the operator supplies through a secret file, and a cluster with no principal SHALL refuse every
connection. The group SHALL draw at that first start the secret an unknown name's verifier derives from and hold it
durably, so every core and restart, and from stage 8 every frontend, answers one name with one salt. Creating a principal whose name exists SHALL be refused, naming it; removing the last principal with the
administrator role SHALL be refused. Removing a principal SHALL evict every session registered for it in the same
command, oldest first, carried to the partitions as every eviction is, so no session outlives its principal; a removal
whose evictions would pass the pending evictions' bound SHALL be refused, the principal and its sessions kept.

#### Scenario: The bootstrap principal authenticates
- **WHEN** a cluster starts for the first time with a bootstrap credential file and a client authenticates with it
- **THEN** authentication succeeds, and the metadata group holds that principal without the password

#### Scenario: A duplicate principal is refused
- **WHEN** a principal named `ann` exists and another `ann` is created
- **THEN** the creation is refused with an error naming `ann`

#### Scenario: The last administrator stays
- **WHEN** the only principal with the administrator role is removed
- **THEN** the removal is refused with an error naming it

#### Scenario: A removed principal's sessions are evicted
- **WHEN** `dave` has two sessions and `carol` one, and `dave` is removed
- **THEN** both of `dave`'s sessions are evicted in the removal and a commit through either is refused as evicted, and `carol`'s session stays

#### Scenario: An unknown name keeps one salt across a restart
- **WHEN** a client authenticates as the unknown name `nobody`, the frontend restarts, and a client authenticates as
  `nobody` on another core
- **THEN** both challenges carry one salt, and the metadata group's log holds the secret it derives from

### Requirement: The metadata group registers sessions
The metadata group SHALL record each session the frontend registers, its 128-bit identity and its principal, as one
command, and SHALL answer a session's principal to the frontend's owner check. A registration SHALL be durable before it
is answered. The registry SHALL hold at most `sessions_max` sessions, evicting the least recently registered with no
commit in flight, as the frontend names them (transactions, "Exactly-once commits through sessions"), SHALL evict a
removed principal's sessions with its removal, and SHALL be the only place a session is evicted; the sequencer SHALL carry each eviction to every partition with its next close,
recorded in its durable close record, so each partition purges the session's entry from its table. A session already
registered SHALL be refused, naming it.

#### Scenario: A registered session survives a restart
- **WHEN** a session is registered for `alice` and the group is killed and restarted
- **THEN** the session's principal is still `alice`

#### Scenario: A full registry evicts the oldest session
- **WHEN** the registry holds `sessions_max` sessions and one more is registered
- **THEN** the least recently registered session is evicted, and a commit through it is refused as evicted

### Requirement: The metadata group holds the tenants
The metadata group SHALL hold the tenants, each the engine's unit of isolation: one catalog of numbered versions, one
run of buckets and the grants of its principals. Nothing SHALL read, write or name across tenants; the cluster scope
alone reaches over them, for backup and export. A namespace, a unit of organisation inside a tenant from stage 6, SHALL
have no field in any key, bucket, ID or frame. The cluster's first start SHALL create tenant 0 by a command of the
group, durable before any role serves, and Placement's runs SHALL list one run per tenant the group holds. Further
tenants, and removing a tenant, SHALL take the cluster operation of `SECURITY.md`'s `operate` action, whose command
arrives with stage 7's administration commands, so in this stage only tenant 0 exists and a cluster wipe has no command.
The frontend SHALL refuse a request naming a number the group does not hold (client-endpoint, "Every request naming a
tenant passes one gate"), and a partition SHALL refuse a Bind naming a tenant its placement map lists no run for with
Error 1 (Protocol), the message naming the tenant.

#### Scenario: The first start creates tenant 0
- **WHEN** a frontend starts on a metadata group holding no tenant
- **THEN** the group holds tenant 0 durably before the frontend serves, Placement's runs list it, and a restart creates
  none

#### Scenario: The tenants survive recovery
- **WHEN** the group holds tenant 0, a snapshot is taken, and the group is killed and restarted
- **THEN** the group holds tenant 0 at the same log position, from the snapshot and the commands after it

#### Scenario: A Bind naming an unknown tenant is refused
- **WHEN** a Bind names tenant 9, for which the partition's placement map lists no run
- **THEN** the partition refuses it with Error 1, the message naming tenant 9, and holds no transaction

### Requirement: The metadata group holds a tightening's check
The metadata group SHALL hold a version that tightens a predicate as being checked from its offer, each partition's report durably,
until every partition the placement map gives the tenant at the version's effective epoch has passed it or a restoring version
withdraws it, the withdrawal one command with that version's offer. A report of a version not being checked, and a report repeated,
SHALL change nothing. A partition's report of a version it found nothing to refuse in SHALL read as passed whether the version
tightens anything or not. The version before one being checked SHALL be retained while the check runs.

#### Scenario: A check ends at its last partition's report
- **WHEN** a version being checked is assigned its epoch over two partitions, and each reports its check passed
- **THEN** it is being checked until the second report, a client reads the version before it until then, and a report repeated
  appends nothing

#### Scenario: A withdrawal is one command with the restoring version
- **WHEN** a partition reports the data refuses a version being checked, the group offers the version restoring it, and the group
  restarts
- **THEN** the recovered state holds the first withdrawn and the second pending, and neither is being checked

