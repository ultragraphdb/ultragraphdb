## MODIFIED Requirements

### Requirement: Transactions are strictly serializable
Every history of committed transactions within one partition SHALL be equivalent to a serial order in which a
transaction acknowledged before another began comes first. A transaction SHALL read and write within one partition in
this stage (partitions, "A transaction stays within one partition in this stage").

#### Scenario: Write skew is refused
- **WHEN** two transactions each read `on-call` of `alice` and `bob`, both `true`, and each sets a different one to
  `false`, concurrently, all four facts in one partition
- **THEN** one of them commits and the other aborts with a conflict

### Requirement: Interactive transactions
A client SHALL begin a transaction, send any number of reads and writes in separate requests, and end it with a commit
or an abort. An aborted transaction SHALL write nothing. A transaction SHALL be bound to the partition its first key
reaches.

#### Scenario: An aborted transaction writes nothing
- **WHEN** a transaction inserts `<a> <knows> <b>` in one request and aborts in the next
- **THEN** `<a> <knows> <b>` is not held

## ADDED Requirements

### Requirement: Commits answer a session token
A committed transaction's reply SHALL carry a session token naming its partition, its log position and the epoch of its
commit version; a later read or transaction carrying the token SHALL see that commit and every commit before it in that
partition.

#### Scenario: A token names its commit
- **WHEN** a transaction commits in partition 1 at position 40 with a version of epoch 12
- **THEN** its reply carries the token (partition 1, position 40, epoch 12)

#### Scenario: A read with a token sees the commit
- **WHEN** a client commits `<a> <knows> <b>` and reads `<a>`'s statements with the commit's token from another connection
- **THEN** the read returns `<a> <knows> <b>`
