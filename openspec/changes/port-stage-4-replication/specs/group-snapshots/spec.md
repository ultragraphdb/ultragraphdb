## ADDED Requirements

### Requirement: Snapshots are taken at a safe point
The leader SHALL propose a snapshot when the log exceeds Dgraph's thresholds, at the latest applied index, with a
read timestamp equal to the highest commit timestamp at or below that index; a snapshot at or below the current one
SHALL change nothing. The discard timestamp SHALL follow the snapshot's read timestamp.

#### Scenario: The snapshot timestamp is the last commit below it
- **WHEN** a snapshot is taken at an index whose last commit below it has timestamp 900
- **THEN** the snapshot's read timestamp is 900

#### Scenario: An older snapshot proposal is ignored
- **WHEN** a snapshot proposal names an index at or below the current snapshot
- **THEN** the log and the snapshot are unchanged

### Requirement: A lagging replica is brought up by a stream
A replica needing entries the log no longer holds SHALL receive the leader's store at the snapshot's read
timestamp as a stream, and SHALL record the snapshot only after the data has landed; a replica with an earlier
snapshot SHALL receive only versions since it, and SHALL drop predicates and types the leader no longer has.

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
- **WHEN** the leader starts a snapshot stream for a timestamp it has not applied yet
- **THEN** it waits until it has applied that timestamp before sending data
