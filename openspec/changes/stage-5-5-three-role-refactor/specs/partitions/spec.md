## ADDED Requirements

### Requirement: A storage process hosts partitions over bucket ranges
A storage process SHALL host the partitions the placement map gives it, each with its own log, its own commit step and
its own keyspaces, holding the data of its bucket ranges and nothing else. A partition SHALL be the same code whether a
process hosts one or many. The storage binary SHALL host one partition unless configured otherwise.

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
learns where to send it.

#### Scenario: A write reaches its bucket's partition
- **WHEN** a write's subject falls in bucket 30,000, which partition 2 owns
- **THEN** partition 2 applies it

#### Scenario: A misrouted write is refused with its owner
- **WHEN** a write for bucket 30,000 reaches partition 1
- **THEN** partition 1 refuses it with an error naming bucket 30,000 and partition 2, and writes nothing

### Requirement: A transaction stays within one partition in this stage
A transaction SHALL read and write the buckets of one partition. A transaction that reaches a key of a second partition
SHALL be aborted with the reason `CrossPartition`, naming both partitions, writing nothing; stage 8 executes such
transactions through the sequencer.

#### Scenario: A transaction reaching a second partition aborts
- **WHEN** a transaction writes an entity of partition 1 and then reads an entity of partition 2
- **THEN** the transaction aborts with `CrossPartition` naming partitions 1 and 2, and holds nothing

### Requirement: Reverse keys carry their source-bucket part
A reverse key SHALL carry, after its object, the part of the source-bucket space its entries come from, part 0 of 1
for every node the placement map does not split; a node the map splits into `k` parts SHALL have its reverse entries
from bucket `b` in part `b · k / 2^24`, rounded down. In this stage no node is split, and every reverse key carries part
0 of 1.

#### Scenario: An unsplit node has one part
- **WHEN** an edge to an unsplit object is written
- **THEN** its reverse key carries part 0 of 1

#### Scenario: A split node spreads its reverse entries
- **WHEN** the map splits an object into 4 parts and edges from buckets 0, 4,194,304 and 16,777,215 point to it
- **THEN** their reverse entries carry parts 0, 1 and 3

### Requirement: Absence is proven by a node's predicate lists
Before reading a list of a node that may not exist, a reader SHALL consult the node's subject-predicates list (or
object-predicates list for a reverse read) and SHALL read no list of a predicate that list does not name. A node created
in the reading transaction SHALL read no list at all, as stage 4 established.

#### Scenario: A predicate the node lacks is not read
- **WHEN** a node holds only `name`, and a read asks for its `age`
- **THEN** the read answers no `age` after reading the node's subject-predicates list, and reads no `age` list

### Requirement: The benchmark measures N partitions in one process
`wal_bench` SHALL run N partitions in one process, each on its own pinned compio runtime with its own generator seeded
from the run's seed and producing only commits of its own buckets, and SHALL report each partition's and the total
relations per second, disk bytes and syncs per second.

#### Scenario: The benchmark reports every partition
- **WHEN** `wal_bench` runs 4 partitions for 10 seconds
- **THEN** its report lists 4 partitions' relations per second and their total, and two runs with one seed generate the
  same commits
