## ADDED Requirements

### Requirement: Faults are injected and histories checked
A harness SHALL run three-replica Alpha and Zero groups as processes, kill and restart them, partition them, and
slow their links, while workloads record histories that Elle checks.

#### Scenario: Histories stay serializable under faults
- **WHEN** list-append and register workloads in both transaction forms run while the harness kills Alphas and
  Zeros and partitions the network
- **THEN** Elle finds no anomaly in the recorded histories

#### Scenario: No acknowledged write is lost
- **WHEN** a workload records every acknowledged write while the harness kills and restarts replicas
- **THEN** after the faults stop and the group recovers, every acknowledged write is readable

### Requirement: A partitioned leader does not commit
A leader cut off from its quorum SHALL commit nothing, and after the partition heals it SHALL rejoin without
forcing a new term.

#### Scenario: An isolated leader rejoins quietly
- **WHEN** a group's leader is partitioned, a new leader is elected, and the partition heals
- **THEN** the old leader committed nothing while isolated and rejoins as a follower without raising the term
