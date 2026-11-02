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

### Requirement: Exactly-once commits through sessions
A client SHALL register a session through the frontend, which draws its 128-bit identity from its injected random source
and records it with the registering principal in the metadata group, one command per session and none per commit. A
session SHALL belong to that principal: the frontend SHALL check every commit's principal against the registry before
routing it, and only that principal SHALL commit through the session or read its stored replies. A client SHALL send
each commit with its session and a request number above the session's last, from one counter per session, since the
client does not know which partition a commit reaches. Each partition SHALL keep its own bounded session table,
recording a session at its first commit there with the last request number committed there and its stored reply; a
partition SHALL refuse a request number below its last for the session as out of order, and numbers the session
committed in other partitions leave gaps it accepts. A retry of a partition's last committed request SHALL return its
stored reply without committing again; a retry of a request the partition does not hold, with no transaction open for
it, SHALL be refused as not committed; a retry the frontend cannot route SHALL be asked of every partition and SHALL
answer the stored reply of the one holding it, or not committed when none does. A session SHALL have at most one commit
in flight. A request of a session evicted from the registry or from a partition's table SHALL be refused with an error
saying so.

#### Scenario: A retried commit commits once
- **WHEN** a client commits request 7 of its session, loses the reply, and sends request 7 again
- **THEN** it receives the same reply and commit version, and the transaction's writes are held once

#### Scenario: A session commits through two partitions
- **WHEN** a session commits request 1 in partition 1, request 2 in partition 2 and request 3 in partition 1, then loses
  the reply to request 3 and sends it again on a new connection
- **THEN** all three commit once, partition 1 accepts request 3 after its last, 1, and the retry receives request 3's
  stored reply from partition 1

#### Scenario: An evicted session is told so
- **WHEN** a session is evicted from the full table and its client sends its next request
- **THEN** the request is refused with an error saying the session was evicted

#### Scenario: A retry of a request never committed is told so
- **WHEN** a client sends request 8 of its session again, with no transaction open for it, and the session's last
  committed request is 7
- **THEN** the request is refused as not committed, `detail` 8, and nothing is committed

#### Scenario: A session belongs to its principal
- **WHEN** principal `alice` registers a session, and a connection authenticated as `bob` commits through it in a
  partition where `alice` has not yet committed
- **THEN** the commit is refused with code 21, `detail` 2, nothing is committed, and `alice`'s next request through the
  session is served

### Requirement: Aborts name their reason
An abort SHALL carry one of: conflict, with the kind of read that conflicted; too old, when the transaction's read
version is below what validation still holds; past its limit; busy, when the commit queue is full; too large, when its
write set exceeds the Welcome's largest write set, as sent or as its commit-time operations leave it, the latter
refused when the transaction is decided and taking no commit version (amended 2026-10-30); refused, when the catalog
refuses a write or a term breaks the server's rules; operation failed, when a compare-and-set did not hold, with the
value found; permission denied, when its principal may not do what a request asks; cross-partition, when the
transaction reaches a second partition, naming both (partitions, "A transaction stays within one partition in this
stage"). Conflict, too old, past its limit and busy SHALL be marked retryable; too large, refused, operation failed,
permission denied and cross-partition SHALL not.

#### Scenario: A full commit queue answers busy
- **WHEN** the commit queue is full and a transaction commits
- **THEN** the commit fails with a retryable busy error, and nothing it wrote is held

#### Scenario: A write set over the Welcome's bound is refused
- **WHEN** a transaction's write set grows past the Welcome's `max_write_set` and it commits
- **THEN** the commit fails with a too-large error that is not marked retryable, its `detail` the bound, and nothing it
  wrote is held

#### Scenario: A transaction reaching a second partition is not retryable
- **WHEN** a transaction bound to partition 1 writes a statement whose subject's bucket partition 2 owns
- **THEN** it aborts with a cross-partition error naming partitions 1 and 2, not marked retryable, and nothing it wrote
  is held

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
