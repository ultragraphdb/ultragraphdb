## ADDED Requirements

### Requirement: Cores are divided between workers and the commit step
The server SHALL run the commit step on one core and a worker on each of the remaining cores available to the process,
each worker a compio runtime pinned to its core; with one core available, one worker SHALL share it with the commit
step. A connection SHALL be accepted onto the least-loaded worker and stay
there, and an interactive transaction SHALL run on its connection's worker.

#### Scenario: A node with eight cores runs seven workers
- **WHEN** the server starts with eight cores available
- **THEN** it runs seven workers and one commit step

#### Scenario: A node with one core still serves
- **WHEN** the server starts with one core available
- **THEN** it runs one worker and the commit step on that core, and accepts connections

### Requirement: Queues are bounded
Every queue between workers and the commit step SHALL be bounded; a commit arriving at a full queue SHALL be refused as
busy rather than queued.

#### Scenario: Load beyond the queue is refused, not buffered
- **WHEN** commits arrive faster than the commit step decides them and its queue is full
- **THEN** further commits are refused as busy until the queue drains

### Requirement: Configuration
The server binary SHALL read its configuration once at start: the listen address, the data directory, the transaction
limits, the power-loss-protection flag, and overrides of the round bounds and the checkpoint interval. The cluster
identity SHALL be drawn at random when the data directory is created and kept in the superblock, not configured.
Library crates SHALL take their settings as values and read no environment.

#### Scenario: An invalid setting stops the start
- **WHEN** the server starts with a read-write limit of zero
- **THEN** it exits with an error naming the setting, before opening the store

### Requirement: Start, recovery and shutdown
The server SHALL open the store and the log, run recovery, and only then accept connections; a recovery that fails
SHALL stop the start with its error. On shutdown it SHALL stop accepting, finish or abort open transactions, sync the
log and the store, and exit. Telemetry SHALL be installed once, through `ultragraphdb_common_telemetry`.

#### Scenario: Connections wait for recovery
- **WHEN** the server starts with rounds to replay
- **THEN** no connection is accepted until every round is replayed

#### Scenario: A shutdown loses no acknowledged commit
- **WHEN** the server is shut down while commits are in flight and then restarted
- **THEN** each commit in flight was either acknowledged and is present, or aborted and is absent
