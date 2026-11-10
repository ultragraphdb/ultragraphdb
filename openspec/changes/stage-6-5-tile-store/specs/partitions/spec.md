## ADDED Requirements

### Requirement: A partition's core runs in turns
A partition's core SHALL do its work in turns, each in this order (`specification/storage/STORAGE.md` section 2; T33):
its intake, the requests its role connections handed the core since the turn before, each parsed and none executed;
the commit batch, in which the commit step validates what it gathered and builds a round when one closes (commit-rounds,
"Commits are grouped into rounds"); the lane L, S and X requests, each within `lane_entries_max` entries examined and
`lane_bytes_max` bytes answered in the turn (storage-lanes, "Every read names one of three lanes"); one compaction or
build beat (tile-engine, "Compaction is paced by units the rounds earn"); one checkpoint step (tile-grid, "A checkpoint
drains, publishes and frees"); and one submission of the turn's I/O, the log's append, the grid's writes and the tile
reads of attempts that missed the cache among it. A round's log sync SHALL be issued in a turn and its completion taken
as an input of a later turn, which applies the round and acknowledges its commits, so the turn issuing a sync goes on
to its lanes, its beat and its checkpoint step. Other cores and the role connections SHALL reach the core only through
bounded channels that wake it.

#### Scenario: A turn decides its commits before it serves reads
- **WHEN** a turn's intake holds ten commits and a lane S Frontier
- **THEN** the ten commits are decided in the turn's commit batch before the Frontier is served, and the round's append
  is among the I/O the turn submits once, at its end

#### Scenario: A sync completes in a later turn
- **WHEN** a turn issues a round's sync and its intake also holds a lane L request
- **THEN** that turn answers the lane L request without waiting for the sync, and the round is applied and its commits
  acknowledged in the turn that takes the sync's completion

#### Scenario: Nothing executes on intake
- **WHEN** a Write of an open transaction arrives while a turn's commit batch is being decided
- **THEN** the Write is parsed in the next turn's intake and executed after it, and the round being decided holds
  nothing of it

### Requirement: The session table is a record run keyed by the session
Each partition SHALL hold its session table as session records (`STORAGE.md` section 9.3, kind 10, storage class M) in
the replica's own family of tenant 0 and group `ABSENT`, keyed by the session's 16 bytes alone (T48, C3). Each record's
value SHALL hold the session's last request, its commit version, the position of the round that logged it, the share
number for an Ingest share and 0 otherwise, and the reply's command, its 128-byte command header and its body, a Commit,
Drop or Ingest reply, laid out as kind 10's value is. A request committed through a session SHALL replace its session's
record at the request's commit version, and the evictions a round logs SHALL delete their sessions' records at that
round's `first_version`, before its entries (commit-rounds, "A round logs the sessions evicted at it"), so every replica
and replay holds the same table at every position, and a resend finds the session's last request whatever share
number its placement gave it.

#### Scenario: A later request replaces the record
- **WHEN** session `s` commits request 3 and then request 5 in partition 1
- **THEN** partition 1's table holds one record for `s`, naming request 5 with its reply, and a resend of request 3 is
  refused with code 13, `detail` 5

#### Scenario: A Drop's reply is stored whole
- **WHEN** a Drop of `p` through session `s` commits in partition 1, and the client resends the Drop
- **THEN** `s`'s record holds the Drop reply's command, command header and body, and the resend receives that reply byte
  for byte

#### Scenario: An Ingest share's record names its share
- **WHEN** an Ingest share numbered 2 commits through session `s` in partition 1
- **THEN** `s`'s record, keyed by `s` alone, holds share 2 and the share's Ingest reply

### Requirement: A partition refuses commits its store cannot admit
A partition's commit step SHALL admit an entry into the round it builds only as its store's admission allows
(tile-engine, "Admission counts each round's own changes"). An entry that does not join SHALL wait for compaction or a
checkpoint to make room; a transaction's Commit still waiting at its transaction's deadline, and a Drop, a one-frame write or
an Ingest share still waiting at `transaction_ms` from its arrival (tile-engine, "Admission counts each round's own
changes"), SHALL be refused with code 5 (Busy), retryable, writing nothing, `detail` 1 when the delta's watermark held it
and 2 when grid admission (`GridFull`) held it (`specification/binary_protocol/PROTOCOL.md` section 9.4). A refused
Drop or one-frame write SHALL be retried as the same request, same session and request number, after a delay, and a
refused Commit's transaction SHALL be run again on a new Begin.

#### Scenario: A refused one-frame write is resent as itself
- **WHEN** a DirectWrite through a session waits at a partition whose grid admission holds until `transaction_ms` from its arrival
- **THEN** it is refused with code 5, `detail` 2, the partition's table holds nothing of it, and its resend with the same
  request number commits once room is freed

#### Scenario: A refused Commit is run again
- **WHEN** a transaction's Commit waits at a partition whose delta stands at its watermark until its deadline
- **THEN** the Commit is refused with code 5, `detail` 1, nothing it wrote is held, and the transaction run again on a
  new Begin commits once compaction has made room

## MODIFIED Requirements

### Requirement: A storage process hosts partitions over bucket ranges
A storage process SHALL host the partitions the placement map gives it, each with its own log, its own commit step and
its own tile store engine over a grid of its own (tile-grid, "The grid file"), holding the data of its bucket ranges and
nothing else. An entity's home SHALL be the partition the placement map gives its bucket. A partition SHALL be the same
code whether a process hosts one or many. The storage binary SHALL host one partition unless configured otherwise. The
frontend SHALL form the placement when the metadata group holds none, an even map over `[cluster] partition_count`. A
storage process SHALL claim the partitions `[storage] partition_first` and `partition_count` name, and any number of
processes SHALL host the partitions between them; a claim on a partition outside the placement, or one another address
holds, SHALL be refused. In this stage the placement SHALL then stay fixed, a process holding its map for its lifetime.

#### Scenario: The frontend forms the placement
- **WHEN** a frontend with `partition_count` 4 starts on a metadata group holding no placement
- **THEN** the metadata group holds an even map over 4 partitions, and a restart forms none

#### Scenario: Two storage processes share the partitions
- **WHEN** one storage process claims partitions 1 and 2 and another claims 3 and 4
- **THEN** both join, Members names each partition's address, and a write reaches its partition on either process

#### Scenario: A claim on a held partition is refused
- **WHEN** one storage process holds partition 2 and another claims it
- **THEN** the claim is refused with an error naming partition 2 and its holder, and the placement is unchanged

#### Scenario: A restart claims its partitions again
- **WHEN** a storage process holding partitions 1 and 2 restarts at the same addresses
- **THEN** its claim succeeds

#### Scenario: Each partition has a grid of its own
- **WHEN** a storage process hosting partitions 1 and 2 as replica 1 has started
- **THEN** its data directory holds `p1/r1/grid` and `p2/r1/grid`, each `extent_bytes × grid_extents` bytes

### Requirement: A write reaches its log without copying its terms
A partition's host SHALL apply a write from the frame that carried it, its terms borrowed from the frame until each is
resolved to its ID, with no owned copy of a term made per change, whatever a term's first resolution stores. The log
SHALL hold the bytes the record was encoded to, and a replay and stage 8's replication SHALL read those bytes where they
lie. Applying a frame and applying the same changes as a `Changeset` SHALL give the same record.

#### Scenario: A frame and a changeset give one record
- **WHEN** the same changes are applied once from a Write frame and once as a `Changeset`
- **THEN** the two write-set records are byte-identical

#### Scenario: Two partitions keep their data apart
- **WHEN** a storage process hosts partitions 1 and 2, and an entity of each partition's buckets is written
- **THEN** each entity's entries are held only in its own partition's delta and grid, and each partition's log holds
  only its own rounds

#### Scenario: The default is one partition
- **WHEN** the storage binary starts with no partition count configured
- **THEN** it hosts one partition owning every bucket

### Requirement: A partition owns every entry its subjects' writes derive
A partition SHALL own its home entities' state and every entry derived from the statements its own subjects write:
forward rows, the reverse rows naming its own subjects under each object, index and count entries, statement records and
statistics records (`STORAGE.md` sections 15.2 to 15.4). It SHALL write all of them in the one local commit that writes
the statement, whichever partition owns the statement's object, so an edge's forward and reverse entries are always
consistent. No partition SHALL write an entry another partition's statement derives.

#### Scenario: An edge to another partition's entity stays with its subject
- **WHEN** `<a>`, homed in partition 1, gains `<a> <knows> <b>`, and `<b>` is homed in partition 2
- **THEN** partition 1 holds the forward entry of `knows` for `a` and its own reverse entry of `knows` for `b` naming
  `a`, both written in one commit, and partition 2's delta and grid gain no entry of the statement

#### Scenario: A hub's inbound entries spread over its referrers
- **WHEN** subjects homed in partitions 1, 2 and 3 each gain an edge to `<hub>`, homed in partition 1
- **THEN** each partition's reverse row of `<hub>` names only its own subjects, and together they name every subject

### Requirement: An entity's referrer set names where its inbound edges may be
The home of every entity `o` SHALL keep its referrer set `R(o)`: a superset of the subject bucket groups holding edges
to `o`, where a bucket group is 64 contiguous buckets, the bucket's top 18 bits, so 2^10 groups per tenant run. Each
member SHALL carry the epoch it was added at and a generation, and `R(o)` SHALL be held as additive referrer-set records
(`STORAGE.md` section 9.3, kind 7) at the commit versions that add or renew its members, whose fold is the union by
group keeping each group's greatest generation and earliest epoch, so a read at an epoch sees the members added at or
below it. A member SHALL be added when the home accepts a registration (term-dictionary, "A reference to another
partition's entity registers at its home"), admitted into the home's commit step with a commit version of its own, and,
for a group the home itself holds, in the home's own commit that first references `o` from that group, at generation 1.
A registration naming the member's current generation, or naming any generation for a group not in `R(o)`, SHALL add or
renew the member at one generation above the larger of the generation named and the highest generation the group ever
held in `R(o)`; a registration naming a generation below the member's current one SHALL change nothing and SHALL be
answered with the current generation. A bucket group SHALL resolve to the partitions holding its buckets through the
placement map in effect at a read's epoch, so a move or split of bucket ranges rewrites no member. In this stage `R(o)`
SHALL only grow; a member is removed only by the reclamation rule (transaction-verification, "A Lean model of
placement"), which stage 8 builds.

#### Scenario: A first registration adds a member
- **WHEN** `R(o)` holds no member for bucket group 7 and the home accepts, at epoch 12, a registration for group 7
  naming generation 0
- **THEN** `R(o)` holds group 7 with epoch 12 and generation 1, and a read of `R(o)` at epoch 11 does not name it

#### Scenario: A repeated registration changes nothing
- **WHEN** `R(o)` holds group 7 at generation 3, and a registration for group 7 naming generation 2 arrives
- **THEN** `R(o)` still holds group 7 at generation 3 with its epoch, and the reply names generation 3

#### Scenario: A renewal raises the generation
- **WHEN** `R(o)` holds group 7 at generation 3, and a registration for group 7 naming generation 3 arrives
- **THEN** `R(o)` holds group 7 at generation 4 with its earlier epoch, and the reply names generation 4

#### Scenario: The home adds its own group without a registration
- **WHEN** a subject of bucket group 3, homed with `<hub>` in partition 1, gains an edge to `<hub>` in a commit at
  epoch 8
- **THEN** `R(hub)` at epoch 8 names group 3, added by that commit, and no registration is sent

#### Scenario: A group resolves through the map at the read's epoch
- **WHEN** group 7's buckets belong to partition 2 up to epoch 20 and to partition 3 from epoch 21, and `R(o)` names
  group 7
- **THEN** a reverse read of `o` at epoch 20 asks partition 2 for group 7 and one at epoch 21 asks partition 3, and
  `R(o)` is unchanged by the move

### Requirement: A commit under a registration reply reads its mark
A write committed under a registration's reply SHALL read the mark of its `(o, g)` (`STORAGE.md` section 9.3, kind 6)
and record the mark's conflict key (`PROTOCOL.md` section 12.2, conflict family 6) in its read set, so a renewal or a
reclamation report of that member committed after the reply and before the write conflicts with it and the write
aborts, retryable, rather than marking `(o, g)` at a generation the home no longer holds.

#### Scenario: A renewal between the reply and the commit conflicts
- **WHEN** transaction T in partition 1 receives a reply naming generation 4 for `(o, 7)`, and another transaction of
  partition 1 renews `(o, 7)` to generation 5 and commits before T
- **THEN** T aborts with a conflict, writes nothing, and its retry marks `(o, 7)` at generation 5

### Requirement: A delete by pattern reaching another partition is refused in this stage
A delete by pattern SHALL remove the entries the pattern names in the partition owning its subject, the reverse, index,
count and statistics entries derived from them included, in one local commit. A delete by pattern whose subject another
partition owns, or that would remove entries other partitions' subjects wrote, such as a node's inbound edges, SHALL be
aborted with `CrossPartition`, naming the partitions, and SHALL write nothing. Stage 8 executes such deletes through the
sequencer.

#### Scenario: A delete by pattern of another partition's subject is refused
- **WHEN** a transaction bound to partition 1 deletes the pattern `<b>`, `knows`, and `<b>` is homed in partition 2
- **THEN** the transaction aborts with `CrossPartition` naming partitions 1 and 2, and every `knows` of `<b>` is still
  held

#### Scenario: A delete by pattern leaves other partitions' edges
- **WHEN** `<a>`, homed in partition 1, holds `<a> <knows> <b>`, and a transaction in partition 2, `<b>`'s home, deletes
  the pattern `<b>`, `name`
- **THEN** it commits in partition 2 alone, `<b>` holds no `name`, and `<a> <knows> <b>` is still held

## REMOVED Requirements

### Requirement: Absence is proven by a node's predicate lists
**Reason**: The tile store keeps no subject-predicates or object-predicates lists. A row is read by its logical key from
its family's tile and the delta, each tile's filter skipping a row it does not hold (`STORAGE.md` sections 4.5, 7.3 and
18.2), and a read of every predicate of a kind names the node's predicates itself (`PROTOCOL.md` section 13.21).
**Migration**: Reads by row and by every predicate are storage-lanes' "A Frontier answers named rows in request order
across groups and classes" and "A Frontier may name every predicate of a kind"; a transaction's reads of rows it wrote
merge its overlay, local-commit's "Reads see two logical overlays".
