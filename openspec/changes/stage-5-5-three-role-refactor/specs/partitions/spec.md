## ADDED Requirements

### Requirement: A storage process hosts partitions over bucket ranges
A storage process SHALL host the partitions the placement map gives it, each with its own log, its own commit step and
its own keyspaces, holding the data of its bucket ranges and nothing else. An entity's home SHALL be the partition the
placement map gives its bucket. A partition SHALL be the same code whether a process hosts one or many. The storage
binary SHALL host one partition unless configured otherwise. The frontend SHALL form the placement when the metadata group
holds none, an even map over `[cluster] partition_count`. A storage process SHALL claim the partitions
`[storage] partition_first` and `partition_count` name, and any number of processes SHALL host the partitions between
them; a claim on a partition outside the placement, or one another address holds, SHALL be refused. In this stage the
placement SHALL then stay fixed, a process holding its map for its lifetime.

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
- **THEN** each entity's keys are held only in its own partition's keyspaces, and each partition's log holds only its
  own rounds

#### Scenario: The default is one partition
- **WHEN** the storage binary starts with no partition count configured
- **THEN** it hosts one partition owning every bucket

### Requirement: Requests reach the partition of their bucket
Every write SHALL be routed to the partition owning its subject's bucket, and a partition SHALL refuse a key outside its
bucket ranges with an error naming the bucket and the partition that owns it, so a sender with a stale placement map
learns where to send it. A partition SHALL answer so a first request or one-frame write whose first subject lies outside its
ranges, code 26, and SHALL answer one whose first subject lies inside them and a later one outside with `CrossPartition`, code 23.

#### Scenario: A write reaches its bucket's partition
- **WHEN** a write's subject falls in bucket 30,000, which partition 2 owns
- **THEN** partition 2 applies it

#### Scenario: A misrouted write is refused with its owner
- **WHEN** a write for bucket 30,000 reaches partition 1
- **THEN** partition 1 refuses it with an error naming bucket 30,000 and partition 2, and writes nothing

#### Scenario: A later subject outside the partition is cross-partition
- **WHEN** a DirectWrite's first subject lies in partition 1's ranges and a later one in partition 2's, and it reaches
  partition 1
- **THEN** partition 1 refuses it with code 23, not code 26, and writes nothing

### Requirement: A partition owns every entry its subjects' writes derive
A partition SHALL own its home entities' state and every entry derived from the statements its own subjects write:
forward lists, subject-predicates lists, its reverse index from an object to the partition's own subjects,
object-predicates entries, count indexes, value indexes and statistics. It SHALL write all of them in the one local
commit that writes the statement, whichever partition owns the statement's object, so an edge's forward and reverse
entries are always consistent. No partition SHALL write an entry another partition's statement derives.

#### Scenario: An edge to another partition's entity stays with its subject
- **WHEN** `<a>`, homed in partition 1, gains `<a> <knows> <b>`, and `<b>` is homed in partition 2
- **THEN** partition 1 holds the forward entry of `knows` for `a`, its own reverse entry of `knows` for `b` naming `a`,
  and the object-predicates entry of `b`, all written in one commit, and partition 2's keyspaces gain no entry of the
  statement

#### Scenario: A hub's inbound entries spread over its referrers
- **WHEN** subjects homed in partitions 1, 2 and 3 each gain an edge to `<hub>`, homed in partition 1
- **THEN** each partition's reverse index of `<hub>` names only its own subjects, and together they name every subject

### Requirement: An entity's referrer set names where its inbound edges may be
The home of every entity `o` SHALL keep its referrer set `R(o)`: a superset of the subject bucket groups holding edges
to `o`, where a bucket group is 64 contiguous buckets, the bucket's top 18 bits, so 2^10 groups per tenant run. Each
member SHALL carry the epoch it was added at and a generation, and `R(o)` SHALL be versioned as any list is, so a read
at an epoch sees the members added at or below it. A member SHALL be added when the home accepts a registration
(term-dictionary, "A reference to another partition's entity registers at its home"), admitted into the home's commit
step with a commit version of its own, and, for a group the home itself holds, in the home's own commit that first
references `o` from that group, at generation 1. A registration naming the member's current generation, or naming any
generation for a group not in `R(o)`, SHALL add or renew the member at one generation above the larger of the
generation named and the highest generation the group ever held in `R(o)`; a
registration naming a generation below the member's current one SHALL change nothing and SHALL be answered with the
current generation. A bucket group SHALL resolve to the partitions holding its buckets through the placement map in
effect at a read's epoch, so a move or split of bucket ranges rewrites no member. In this stage `R(o)` SHALL only grow;
a member is removed only by the reclamation rule (transaction-verification, "A Lean model of placement"), which stage 8
builds.

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
A write committed under a registration's reply SHALL read the mark key of its `(o, g)` in the marks keyspace, recorded
in its read set, so a renewal or a reclamation report of that member committed after the reply and before the write
conflicts with it and the write aborts, retryable, rather than marking `(o, g)` at a generation the home no longer
holds.

#### Scenario: A renewal between the reply and the commit conflicts
- **WHEN** transaction T in partition 1 receives a reply naming generation 4 for `(o, 7)`, and another transaction of
  partition 1 renews `(o, 7)` to generation 5 and commits before T
- **THEN** T aborts with a conflict naming the mark key, writes nothing, and its retry marks `(o, 7)` at generation 5

### Requirement: A registration's epoch is at most the epoch of every write referencing it
On every write path, the epoch at which the home adds or renews a member of `R(o)` SHALL be at most the commit epoch of
every write that references `o` through that member's group. A home SHALL stamp a registration at its open epoch, one
above the latest close delivered to it, and SHALL reply with that latest close `c`; the registrant SHALL commit a
write referencing `o` only after close `c` has been delivered to it, so it stamps that write at `c + 1` or later
(commit-rounds, "Commit versions are built from the epoch"). A sequenced submission SHALL carry the largest reply epoch
`c + 1` among its registrations, and the sequencer SHALL place it at or above that epoch (sequencer, "The sequencer
orders multi-partition transactions in its log").

#### Scenario: The registrant waits for the home's close
- **WHEN** partition 1, whose latest delivered close is 9, registers a reference to an entity homed in partition 2,
  whose latest delivered close is 11, and the reply names close 11
- **THEN** partition 2 adds `R(o)`'s member at epoch 12, and partition 1 commits the referencing write only after close
  11 is delivered to it, at epoch 12 or later

#### Scenario: A reverse read at a closed epoch misses no edge
- **WHEN** partitions 1 and 2 each commit an edge to `o` at epochs 14 and 15, and a reverse read of `o` runs at epoch 15
  once every partition has closed it
- **THEN** `R(o)` at epoch 15 names both partitions' groups, and the read returns both subjects

### Requirement: A read-write transaction stays within one partition in this stage
A read-write transaction SHALL read and write only what its partition owns. It SHALL bind to the partition owning the
first subject it reads or writes; when its first operation claims a value by an insert-if-absent or an upsert-and-link
and names no subject but new blank nodes, to the partition owning the claimed value's bucket; when its first write names
only new blank nodes, to a partition the frontend chooses in turn (term-dictionary, "IRIs and blank nodes get system IDs");
and when its first operation names no subject otherwise, to the partition owning the tenant's whole bucket run if one does, that first operation aborting it with
`CrossPartition` when none does. It SHALL register its references to entities other partitions own as idempotent
pre-steps outside it. A reverse read, or a read that fans out (a value-index lookup, a scan, a pattern with an unbound
subject, a bound literal object), SHALL be allowed only when the transaction's partition owns the tenant's whole run,
as it always does with one partition; it then reads the partition's own entries, the referrer set included, at its
latest state. Cross-partition work SHALL be aborted with the reason `CrossPartition`, naming the transaction's partition
and the partitions the operation would reach, and writing nothing: a forward read of a subject another partition owns; a
reverse or fan-out read when the partition does not own the tenant's whole run; writing a statement whose subject
another partition owns; a delete by pattern reaching another partition's state ("A delete by pattern reaching another
partition is refused in this stage"); a write of a unique predicate's value whose bucket (system-identity, "The bucket
comes from the tenant and the key") another partition owns; and any Drop reaching several partitions, of a predicate,
a graph, a tenant's data or everything in a tenant. A read-only transaction reads across partitions at its level (quad-reads).
Stage 8 executes cross-partition work through the sequencer.

#### Scenario: A transaction reaching a second partition aborts
- **WHEN** a transaction writes a statement whose subject partition 1 owns and then writes one whose subject partition
  2 owns
- **THEN** the transaction aborts with `CrossPartition` naming partitions 1 and 2, and holds nothing

#### Scenario: Write skew across partitions is refused, not committed
- **WHEN** `alice` is homed in partition 1 and `bob` in partition 2, both `on-call` `true`, and a read-write transaction
  reads `on-call` of `alice` and then of `bob`
- **THEN** the second read aborts the transaction with `CrossPartition` naming partitions 1 and 2, and it writes
  nothing

#### Scenario: A reverse read over a spread tenant aborts a read-write transaction
- **WHEN** a tenant's run spans partitions 1 and 2, `<b>` is homed in partition 2, and a read-write transaction bound
  to partition 1 reads the subjects of `knows` pointing at `<b>`
- **THEN** the transaction aborts with `CrossPartition` naming partitions 1 and 2

#### Scenario: One partition reads its own reverse entries
- **WHEN** one partition owns the tenant's whole run, and a read-write transaction reads the subjects of `knows`
  pointing at `<b>` and then writes `<x> <seen> true`
- **THEN** the read answers from the partition's own reverse entries at its latest state, and the transaction commits

#### Scenario: A first read naming no subject over a spread tenant aborts
- **WHEN** a tenant's run spans partitions 1 and 2, and a read-write transaction's first operation looks up the
  `term` index of `name`
- **THEN** the transaction aborts with `CrossPartition` naming partitions 1 and 2, and holds nothing

#### Scenario: A unique value owned by another partition is refused
- **WHEN** `epi.id` is declared unique, the bucket of `"x"` under `epi.id` belongs to partition 2, and a transaction
  bound to partition 1 insert-if-absents a node with `epi.id` `"x"`
- **THEN** the transaction aborts with `CrossPartition` naming partitions 1 and 2, and no node holds `epi.id` `"x"`

#### Scenario: A plain write of a unique value owned by another partition is refused
- **WHEN** `epi.id` is declared unique, the bucket of `"x"` under `epi.id` belongs to partition 2, and a transaction
  bound to partition 1 writes `<a> <epi.id> "x"`
- **THEN** the transaction aborts with `CrossPartition` naming partitions 1 and 2, and `<a>` holds no `epi.id`

#### Scenario: A first insert-if-absent binds to its claim's partition
- **WHEN** `epi.id` is declared unique, partition 2 owns the bucket of `"x"`, and a read-write transaction's first
  operation is an insert-if-absent of a node with `epi.id` `"x"`
- **THEN** the transaction binds to partition 2 and commits there

#### Scenario: A reference to another partition's entity stays in one partition
- **WHEN** a transaction bound to partition 1 writes `<a> <knows> <b>`, `<b>` homed in partition 2
- **THEN** it registers the reference at partition 2, commits in partition 1 alone, and does not abort

#### Scenario: A Drop over several partitions is refused
- **WHEN** a tenant's buckets span partitions 1 and 2, and a client drops the tenant's data
- **THEN** the Drop is refused with `CrossPartition` naming partitions 1 and 2, and nothing is dropped

### Requirement: A delete by pattern reaching another partition is refused in this stage
A delete by pattern SHALL remove the entries the pattern names in the partition owning its subject, the reverse,
predicate-list, index and statistics entries derived from them included, in one local commit. A delete by pattern
whose subject another partition owns, or that would remove entries other partitions' subjects wrote, such as a node's
inbound edges, SHALL be aborted with `CrossPartition`, naming the partitions, and SHALL write nothing. Stage 8 executes
such deletes through the sequencer.

#### Scenario: A delete by pattern of another partition's subject is refused
- **WHEN** a transaction bound to partition 1 deletes the pattern `<b>`, `knows`, and `<b>` is homed in partition 2
- **THEN** the transaction aborts with `CrossPartition` naming partitions 1 and 2, and every `knows` of `<b>` is still
  held

#### Scenario: A delete by pattern leaves other partitions' edges
- **WHEN** `<a>`, homed in partition 1, holds `<a> <knows> <b>`, and a transaction in partition 2, `<b>`'s home, deletes
  the pattern `<b>`, `name`
- **THEN** it commits in partition 2 alone, `<b>` holds no `name`, and `<a> <knows> <b>` is still held

### Requirement: Absence is proven by a node's predicate lists
Before reading a list of a node that may not exist, a reader SHALL consult the node's subject-predicates list (or, for
a reverse read, the object-predicates list each asked partition holds for it) and SHALL read no list of a predicate
that list does not name. A node created in the reading transaction SHALL read no list at all, as stage 4 established.

#### Scenario: A predicate the node lacks is not read
- **WHEN** a node holds only `name`, and a read asks for its `age`
- **THEN** the read answers no `age` after reading the node's subject-predicates list, and reads no `age` list

### Requirement: The benchmark measures N partitions in one process
`wal_bench` SHALL run N partitions in one process, each on its own pinned compio runtime with its own generator seeded
from the run's seed and producing only commits whose subjects are of its own buckets, with a configured share of edges
pointing at entities other partitions own, so registration runs; it SHALL report each partition's and the total
relations per second, disk bytes and syncs per second, and the registrations per second and their wait.

#### Scenario: The benchmark reports every partition
- **WHEN** `wal_bench` runs 4 partitions for 10 seconds
- **THEN** its report lists 4 partitions' relations per second and their total, and two runs with one seed generate the
  same commits

#### Scenario: The benchmark exercises registration
- **WHEN** `wal_bench` runs 4 partitions with 10% of edges pointing at other partitions' entities
- **THEN** its report lists registrations per second and the wait a first reference costs, and a reverse read, at the
  run's last closed epoch, of each entity that received an edge from another partition returns the subjects the same
  seed's run on one partition returns
