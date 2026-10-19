## ADDED Requirements

### Requirement: A transaction spanning groups is all or nothing
A transaction writing predicates in several groups SHALL be visible in every group or in none, at every read
timestamp, and SHALL abort in every group if it fails in one.

#### Scenario: Both groups show the commit at the same timestamps
- **WHEN** a transaction sets `color` (group 1) and `shape` (group 2) of one node, and reads run at every timestamp
  around its commit
- **THEN** each read sees both values or neither

#### Scenario: A failure in one group aborts the whole transaction
- **WHEN** a transaction's writes in group 2 are refused
- **THEN** group 1 keeps none of its writes either, and the client receives the failure

### Requirement: Conflicts and stale owners abort across groups
Concurrent transactions writing one key SHALL commit at most one, under each transaction form, and a transaction
whose writes were built for a group that no longer owns the predicate SHALL abort.

#### Scenario: One of two cross-group writers to a key commits
- **WHEN** two transactions spanning groups 1 and 2 both set `color` of one node from the same start timestamp
- **THEN** exactly one commits

#### Scenario: A transaction built for a moved predicate aborts
- **WHEN** a transaction writes `color` while `color` is moved to another group
- **THEN** it is aborted

### Requirement: Upsert blocks spanning groups
An upsert block whose query or mutations touch several groups SHALL keep stage 3's guarantee: its conditions see
every commit ordered before it, and it does not abort for a conflict.

#### Scenario: A cross-group upsert creates one node
- **WHEN** two upsert blocks concurrently query `email` (group 1) and create a node with `email` and `name`
  (group 2) only if none exists
- **THEN** exactly one node exists afterwards
