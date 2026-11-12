## MODIFIED Requirements

### Requirement: The metadata group is a log-backed state machine with one replica
The metadata group's state SHALL be a function of its log: every command SHALL be appended and applied in log order,
acknowledged only once durable, and recovery SHALL rebuild the state by replaying the log from the last snapshot. It
SHALL be a state machine of the system group, hosted by a storage process (system-group, "One storage instance hosts the
system group"), with one replica until stage 8; its interface SHALL be the one stage 8 replicates. Reads of its state
SHALL name the log position they read at.

#### Scenario: Recovery rebuilds the state
- **WHEN** the group applies a catalog version, two block leases, a placement change and a principal, and is killed and
  restarted
- **THEN** its state after recovery equals its state before the kill, read at the same log position

#### Scenario: A snapshot shortens recovery
- **WHEN** a snapshot is taken at position 100 and 20 more commands are applied before a restart
- **THEN** recovery loads the snapshot and replays exactly the 20 commands after it

### Requirement: Catalog versions take effect at an epoch boundary
The metadata group SHALL hold each tenant's catalog as numbered versions whose contents it treats as opaque bytes. It
SHALL commit a new version as pending; at its next close `c + 1` the sequencer SHALL assign it the effective epoch `c +
2` in the close's durable record and announce it with that close (sequencer, "Closes announce the catalog versions
taking effect after them"), and the group SHALL learn the epoch from that close, so every partition holds the version
before it can stamp its effective epoch. Every partition SHALL validate and apply a round against the version in effect
at the round's epoch. The group SHALL retain every version a partition's untrimmed log may name, and a partition SHALL
load those versions before it replays its log. A version whose predecessor is not the tenant's current version SHALL be
refused, naming both. Beside the opaque versions, the group SHALL hold default-entry records keyed by (tenant,
predicate), each the default entry a predicate's first write asks for (schema-storage, "Writes to an unknown predicate
create its schema"): recording one SHALL find a record that exists, SHALL create no catalog version, which the
predecessor check therefore ignores, and SHALL stale no prepared statement and abort no transaction. A default-entry
record SHALL outlive a drop, and Describe names it as recorded. A catalog version SHALL hold at most 523,000 bytes, what
one Catalog frame holds at the floor of `max_frame`: the system group SHALL refuse a Declare that would build a larger
one with code 7, `detail` 523,000, the message naming its size, the catalog unchanged, which the frontend passes to the
client at its Commit; an Install compiling to one is refused the same way from stage 6.6; and the group SHALL refuse to
hold one.

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

### Requirement: Principals live in the metadata group
The metadata group SHALL hold the cluster's principals: each a name and its SCRAM-SHA-256 credential (salt, iteration
count, stored key and server key), never the password. The first principal SHALL be created at the cluster's first start
from a credential the operator supplies through a secret file, which the system group's host reads from `[metadata]
bootstrap_secret_file`, and a cluster with no principal SHALL refuse every connection. The group SHALL draw at that
first start the secret an unknown name's verifier derives from and hold it durably, so every core, every restart and
every process holding a client endpoint answers one name with one salt. A principal's stored key and server key SHALL
never leave the group's host: a client's proof SHALL be checked there, asked by Challenge and Verify (system-group, "A
client's proof is checked in the system group"). Creating a principal whose name exists SHALL be refused, naming it;
removing the last principal with the administrator role SHALL be refused. Removing a principal SHALL evict every session
registered for it in the same command, oldest first, carried to the partitions as every eviction is, so no session
outlives its principal; a removal whose evictions would pass the pending evictions' bound SHALL be refused, the
principal and its sessions kept.

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
- **WHEN** a client authenticates as the unknown name `nobody`, the system group's host and the frontend restart, and a client authenticates as
  `nobody` on another core
- **THEN** both challenges carry one salt, and the metadata group's log holds the secret it derives from

### Requirement: The metadata group registers sessions
The metadata group SHALL record each session a Session command registers, its 128-bit identity and its principal, as one
command, and SHALL decide the owner check a Check asks from the session's principal (system-group, "The registry is
asked by Session, Check and Ended"). A registration SHALL be durable before it is answered. The registry SHALL hold at
most `sessions_max` sessions, evicting the least recently registered with no commit in flight, as a passed Check of kind 2 marks them and an Ended ends the mark, or twice `transaction_ms` after the asking connection's close (transactions, "Exactly-once commits through
sessions"), SHALL evict a removed principal's sessions with its removal, and SHALL be the only place a session is
evicted; the sequencer SHALL carry each eviction to every partition with its next close, recorded in its durable close
record, so each partition purges the session's entry from its table. A session already registered SHALL be refused,
naming it.

#### Scenario: A registered session survives a restart
- **WHEN** a session is registered for `alice` and the group is killed and restarted
- **THEN** the session's principal is still `alice`

#### Scenario: A full registry evicts the oldest session
- **WHEN** the registry holds `sessions_max` sessions and one more is registered
- **THEN** the least recently registered session is evicted, and a commit through it is refused as evicted

### Requirement: The metadata group holds the tenants
The metadata group SHALL hold the tenants, each the engine's unit of isolation: one catalog of numbered versions, one
run of buckets and the grants of its principals. Nothing SHALL read, write or name across tenants; the cluster scope
alone reaches over them, for backup and export. A namespace, a unit of organisation inside a tenant from stage 6.6,
SHALL have no field in any key, bucket, ID or frame. The cluster's first start SHALL create tenant 0 by a command of the
group, durable before the system group listens, and Placement's runs SHALL list one run per tenant the group holds.
Further tenants, and removing a tenant, SHALL take the cluster operation of `SECURITY.md`'s `operate` action, whose
command arrives with stage 7's administration commands, so in this stage only tenant 0 exists and a cluster wipe has no
command. The frontend SHALL refuse a request naming a number Placement's runs do not list (client-endpoint, "Every
request naming a tenant passes one gate"), and a partition SHALL refuse a Bind naming a tenant its placement map lists
no run for with Error 1 (Protocol), the message naming the tenant.

#### Scenario: The first start creates tenant 0
- **WHEN** the system group's host starts on a metadata group holding no tenant
- **THEN** the group holds tenant 0 durably before it listens, Placement's runs list it, and a restart creates
  none

#### Scenario: The tenants survive recovery
- **WHEN** the group holds tenant 0, a snapshot is taken, and the group is killed and restarted
- **THEN** the group holds tenant 0 at the same log position, from the snapshot and the commands after it

#### Scenario: A Bind naming an unknown tenant is refused
- **WHEN** a Bind names tenant 9, for which the partition's placement map lists no run
- **THEN** the partition refuses it with Error 1, the message naming tenant 9, and holds no transaction
