## ADDED Requirements

### Requirement: Snapshots are taken at a safe point
The leader SHALL check once a minute and propose a snapshot when none exists, when the log holds more than Dgraph's
bound (4 WAL files in Dgraph, a size bound on the log's segments), or when 30 minutes have passed and the applied index is 10,000
entries past the first; the snapshot SHALL be at the latest applied index, since no pending transactions bound it, with
a read timestamp equal to the highest commit timestamp applied at or below that index. A snapshot at or below the
current one SHALL change nothing. The discard timestamp SHALL follow the snapshot's read timestamp, bounded by the
oldest open read.

#### Scenario: The snapshot timestamp is the highest commit below it
- **WHEN** a snapshot is taken at an index whose highest commit timestamp at or below it is 900
- **THEN** the snapshot's read timestamp is 900

#### Scenario: An older snapshot proposal is ignored
- **WHEN** a snapshot proposal names an index at or below the current snapshot
- **THEN** the log and the snapshot are unchanged

#### Scenario: An open read holds the discard timestamp below the snapshot
- **WHEN** a snapshot with read timestamp 900 is applied while a read at 700 is open
- **THEN** the discard timestamp stays at or below 700 until the read closes

### Requirement: A lagging replica is brought up by a stream
A replica needing entries the log no longer holds SHALL receive, from the node named in the snapshot's context, that
node's store at the snapshot's read timestamp as a stream, and SHALL record the snapshot only after the data has
landed. A full stream SHALL first clear the follower's store; a follower with an earlier snapshot SHALL receive only
the versions at or above its snapshot's timestamp, with every schema and type key, and SHALL drop predicates and types
the sender no longer has. A stream SHALL not block the Raft thread holding the replica's other groups.

#### Scenario: A far-behind replica converges
- **WHEN** a follower is stopped, the leader's log is compacted past it, and the follower restarts
- **THEN** it receives a snapshot stream and its store equals the leader's at the snapshot's timestamp

#### Scenario: An incremental stream removes stale predicates
- **WHEN** a follower with an earlier snapshot catches up after a predicate was dropped
- **THEN** it receives only newer versions, and the dropped predicate is gone from its store

#### Scenario: An interrupted stream is retried
- **WHEN** a snapshot stream breaks part way
- **THEN** the follower retries, and records the snapshot only after a complete stream

#### Scenario: The sender waits for its own applied timestamp
- **WHEN** a node starts a snapshot stream for a timestamp it has not applied yet
- **THEN** it waits until it has applied that timestamp before sending data
