## ADDED Requirements

### Requirement: Faults are injected and histories checked
A harness SHALL run three-replica Alpha and Zero groups as processes, kill and restart them, partition them, and slow
their links, while workloads record Jepsen histories as EDN or JSON, an operation cut by a kill recorded as `:info`,
which `elle-cli` checks.

#### Scenario: Histories stay strict serializable under faults
- **WHEN** list-append and rw-register workloads in the read-set and upsert-block forms run while the harness kills
  Alphas and Zeros and partitions the network
- **THEN** `elle-cli` reports `true` for `strict-serializable`, an `unknown` result counting as a failure

#### Scenario: Write-key transactions stay snapshot isolated under faults
- **WHEN** the same workloads run as requests checked on their write keys
- **THEN** `elle-cli` reports `true` for `snapshot-isolation`

#### Scenario: No acknowledged write is lost
- **WHEN** a workload records every acknowledged write while the harness kills and restarts replicas
- **THEN** after the faults stop and the group recovers, every acknowledged write is readable

### Requirement: A partitioned leader does not commit
A leader cut off from its quorum SHALL commit nothing, and after the partition heals it SHALL step down on hearing the
new leader without disturbing it.

#### Scenario: An isolated leader rejoins quietly
- **WHEN** a group's leader is partitioned, a new leader is elected, and the partition heals
- **THEN** the old leader committed nothing while isolated, steps down on hearing the new leader, and the new leader
  keeps its term and leadership

### Requirement: A group runs in a deterministic simulator
A group of three replicas and its Zero SHALL run in one process on a seeded cooperative executor over the simulated log
storage, network and clock; the run SHALL be a function of its seed, and after each step the replicated state SHALL be
checked against a shadow model.

#### Scenario: A seed replays a run with crashes and partitions
- **WHEN** a seed whose run crashes a leader and partitions a follower is run twice
- **THEN** both runs commit the same entries in the same order and inject the same faults

#### Scenario: Simulated faults lose no acknowledged write
- **WHEN** a run injects storage faults, crashes, restarts and partitions over many steps
- **THEN** every write acknowledged to a client is applied on every replica that recovers

