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
The server binary SHALL read its configuration once at start: the listen address, the data directory, whether
connections authenticate and the bootstrap secret file, the transaction limits, the power-loss-protection flag, and overrides of the round bounds and the checkpoint interval. The cluster
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

### Requirement: The server authenticates its connections
A server requiring authentication SHALL hold the bootstrap administrator from the secret file its configuration names,
read once when it creates the cluster and kept as its SCRAM-SHA-256 verifier alone, and SHALL create no cluster without
that file. It SHALL refuse a password shorter than 15 characters or on the operator's list of common or breached
passwords. Failed authentications SHALL be throttled per principal and per source address with a growing delay, and
after 100 consecutive failures of a principal its authentications SHALL be refused from every source for the refusal
interval, 15 minutes by default, after which they are accepted again; a success SHALL reset the count. A server
configured without authentication SHALL listen on the loopback interface only, and SHALL refuse to start on any other
address. Each authentication, its outcome and the principal it named SHALL be written to the audit log, and no password,
proof or token SHALL be written to any log (`ultragraphdb_protocol/SECURITY.md`).

#### Scenario: No bootstrap secret, no cluster
- **WHEN** a server requiring authentication starts on an empty data directory without the bootstrap secret file
- **THEN** it exits with an error naming the missing file, and creates no cluster

#### Scenario: A short bootstrap password creates no cluster
- **WHEN** a server requiring authentication starts on an empty data directory with a bootstrap secret file holding a
  password of 14 characters
- **THEN** it exits with an error naming the password rule and not the password, and creates no cluster

#### Scenario: The bootstrap administrator authenticates
- **WHEN** a server requiring authentication creates its cluster from a bootstrap secret file, and a client
  authenticates as the bootstrap administrator with the file's password
- **THEN** the exchange succeeds, and the data directory holds the password's verifier and not the password

#### Scenario: Repeated failures refuse a principal for the interval
- **WHEN** a principal's authentication fails 100 times in a row, and the principal then authenticates with its
  password at once and again after the refusal interval
- **THEN** the first is refused with code 19 and the second succeeds

#### Scenario: A server without authentication listens on loopback only
- **WHEN** a server configured without authentication is given a listen address that is not loopback
- **THEN** it exits with an error naming the setting, before opening the store

#### Scenario: No secret reaches a log
- **WHEN** a client authenticates once with a wrong password and once with the right one
- **THEN** the audit log holds both attempts with their outcomes and the principal named, and no log holds the
  password, a proof or a SCRAM message
