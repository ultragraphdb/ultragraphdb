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
Concurrent requests checked on their write keys that write one key from one read timestamp SHALL commit at most one; a
read-set request SHALL abort when a key or range in its read set was committed after its read timestamp in any group;
a transaction whose writes were built for a group that no longer owns the predicate SHALL abort.

#### Scenario: One of two cross-group writers to a key commits
- **WHEN** two write-key-checked transactions spanning groups 1 and 2 both set `color` of one node, the second starting
  before the first commits
- **THEN** exactly one commits

#### Scenario: A transaction built for a moved predicate aborts
- **WHEN** a transaction's write to `color` is built for group 1, `color` then moves to group 2, and the transaction
  commits after the move
- **THEN** it is aborted

### Requirement: Upsert blocks spanning groups
An upsert block whose query or mutations touch several groups SHALL either keep the guarantee
`one-request-transactions` gives, its conditions seeing every commit ordered before it and no abort for a conflict, or
be refused before it runs with an error naming the groups, as the stage's mechanism decision settles.

#### Scenario: A cross-group upsert creates one node
- **WHEN** two upsert blocks concurrently query `email` (group 1) and create a node with `email` and `name` (group 2)
  only if none exists
- **THEN** exactly one node exists afterwards, or both blocks are refused naming groups 1 and 2
