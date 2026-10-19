## ADDED Requirements

### Requirement: A group of three elects one leader
A group of three replicas SHALL elect exactly one leader within the election timeout (20 ticks of 100 ms), with
PreVote on, and leadership SHALL be readable without blocking the Raft loop.

#### Scenario: One leader is elected at startup
- **WHEN** three replicas of a group start
- **THEN** within about 2 s exactly one reports itself leader and the others name it

#### Scenario: Reading leadership never blocks
- **WHEN** 100,000 leadership checks run concurrently with a busy Raft loop
- **THEN** each returns without waiting on the loop

### Requirement: Writes commit on a quorum
A write SHALL be acknowledged only after a quorum has persisted it, and SHALL be applied by every replica in log
order with the same commit timestamp.

#### Scenario: A write is readable on every replica
- **WHEN** a write commits and each replica is queried at a later timestamp
- **THEN** all three return it

#### Scenario: A write commits with one follower down
- **WHEN** one follower is stopped and a write is sent
- **THEN** the write commits, and the follower holds it after it restarts

#### Scenario: Without a quorum nothing is acknowledged
- **WHEN** two replicas are stopped and a write is sent
- **THEN** the write is not acknowledged, and after a replica returns the write is either applied everywhere or
  nowhere

### Requirement: The apply thread is deterministic
Every replica's apply thread SHALL compute the same outcome for each entry: the same checks, the same upsert-block
results, the same writes, from the entry and the state before it.

#### Scenario: Replicas hold identical state
- **WHEN** a mixed workload of upserts, read-set and plain transactions runs against a group
- **THEN** every replica's store holds the same keys and versions at the same applied index

### Requirement: Proposals are retried and applied once
A proposal SHALL be retried under one key after a timeout, up to three attempts with growing timeouts, and an entry
SHALL be applied once even when it was committed twice; the pending-proposal limit SHALL bound proposals in flight,
and timestamp reports SHALL bypass it.

#### Scenario: A leader crash during a proposal applies it at most once
- **WHEN** the leader crashes after proposing and the proposal is retried on the new leader
- **THEN** its writes are applied exactly once or not at all, and the caller learns which

#### Scenario: A duplicate entry is skipped
- **WHEN** the same proposal key and size are committed twice
- **THEN** the second is skipped and its waiter receives the first's result

#### Scenario: An overloaded group refuses after three attempts
- **WHEN** a proposal times out three times
- **THEN** the caller receives Dgraph's overloaded error and the proposal's limiter slots are released

### Requirement: Restarts resume from the checkpoint
A restarted replica SHALL load its snapshot and log, skip entries at or below its checkpoint index, and reach the
applied index it had.

#### Scenario: A restarted replica reaches its applied index
- **WHEN** a replica is stopped at applied index 5,000 and restarted
- **THEN** it reaches index 5,000 with the same store contents, without reapplying entries at or below its checkpoint

### Requirement: Commit timestamps are leased by the leader
The leader SHALL lease commit timestamps from Zero for the entries it proposes and SHALL carry them in the entries,
and Zero SHALL treat a leased timestamp no entry reports as abandoned after a bounded time.

#### Scenario: A lost proposal does not block reads
- **WHEN** a leader leases timestamps and crashes before its proposal commits
- **THEN** reads at timestamps above the lease proceed once Zero treats the lease as abandoned
