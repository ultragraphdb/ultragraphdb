## ADDED Requirements

### Requirement: A group of three elects one leader
A group of three replicas SHALL elect exactly one leader within twice the election timeout (40 ticks of 100 ms),
with PreVote on and CheckQuorum off, as Dgraph configures etcd raft, and leadership SHALL be readable without
blocking the Raft loop.

#### Scenario: One leader is elected at startup
- **WHEN** three replicas of a group start
- **THEN** within about 5 s exactly one reports itself leader and the others name it

#### Scenario: Reading leadership never blocks
- **WHEN** 100,000 leadership checks run concurrently with a busy Raft loop
- **THEN** each returns without waiting on the loop

### Requirement: Writes commit on a quorum
A write SHALL be acknowledged only after a quorum has persisted it, and SHALL be applied by every replica in log
order with the commit timestamp carried in its entry.

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

### Requirement: Commit timestamps follow the log
The leader SHALL lease commit timestamps from Zero under its group and term, assign them in the order it appends
proposals, and carry them in the entries; the apply thread SHALL abort an entry whose commit timestamp is not above
the last one it applied. Zero SHALL hold a leased timestamp open until the group reports it, and once a new leader
has applied an entry of its own term it SHALL report every timestamp leased under an earlier term and not applied as
abandoned.

#### Scenario: A lost proposal does not block reads
- **WHEN** a leader leases timestamps and crashes before its proposal commits
- **THEN** once the new leader applies an entry of its own term, Zero treats the unapplied leases as abandoned and
  reads above them proceed

#### Scenario: An entry out of timestamp order aborts
- **WHEN** an entry carrying timestamp 150 commits after an entry carrying 200 was applied
- **THEN** every replica aborts it and its timestamp is reported abandoned

### Requirement: Blank nodes take uids leased by the leader
The leader SHALL lease a uid for every blank node of a proposal, upsert blocks included, and carry them in the entry,
so every replica assigns the same uids.

#### Scenario: Replicas assign the same uids
- **WHEN** an upsert block creating three blank nodes is applied by a group of three
- **THEN** each replica stores the three nodes under the same uids

### Requirement: The apply thread is deterministic
Every replica's apply thread SHALL compute the same outcome for each entry, from the entry and the state before it:
the same checks, the same upsert-block results, the same writes, the same deduplication.

#### Scenario: Replicas hold identical state
- **WHEN** a mixed workload of upserts, read-set and plain transactions runs against a group
- **THEN** every replica's store holds the same keys and versions at the same applied index

### Requirement: Reads on a replica wait for its own log
The group leader SHALL propose Zero's done watermark into the group's log, as Dgraph proposes deltas, and a replica
SHALL serve a read at `t` only after applying a watermark entry at or above `t`.

#### Scenario: A lagging follower waits for the commit
- **WHEN** a follower lags its leader by one commit at timestamp `c` and a read at `c` arrives at the follower
- **THEN** the read waits until the follower has applied `c`

### Requirement: Proposals are retried and applied once
A proposal SHALL be retried under one key, carrying the same bytes, its timestamp included, after a timeout, up to
three attempts with timeouts of 4, 8 and 16 s, and only while its entry was not found among committed entries; a
schema proposal SHALL take one attempt of 3 minutes. An entry SHALL be applied once even when committed twice,
decided from the log and the store alone. The pending-proposal limit (256, weighted by attempt) SHALL bound proposals
in flight, and watermark entries and snapshot proposals SHALL bypass it.

#### Scenario: A leader crash during a proposal applies it at most once
- **WHEN** the leader crashes after proposing and the proposal is retried on the new leader
- **THEN** its writes are applied at most once, and a caller that receives an error can read the outcome at a later
  timestamp

#### Scenario: A duplicate entry is skipped on every replica
- **WHEN** the same proposal key is committed twice and the first applied without error
- **THEN** every replica, a restarted one included, skips the second, and its waiter receives success

#### Scenario: An overloaded group refuses after three attempts
- **WHEN** a proposal times out three times
- **THEN** the caller receives `Server overloaded with pending proposals. Please retry later` and the proposal's
  limiter slots are released

### Requirement: Restarts resume from the applied index
The apply thread SHALL write the applied index in the same store batch as the entry's writes, and a restarted
replica SHALL load its snapshot and log and skip entries at or below its applied index. Dgraph instead skips entries
below a checkpoint it records once a minute.

#### Scenario: A restarted replica reaches its applied index
- **WHEN** a replica is stopped at applied index 5,000 and restarted
- **THEN** it reaches index 5,000 with the same store contents, reapplying no entry at or below the index its store
  recorded
