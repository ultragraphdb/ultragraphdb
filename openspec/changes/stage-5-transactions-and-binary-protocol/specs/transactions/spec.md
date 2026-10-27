## ADDED Requirements

### Requirement: Transactions read a snapshot at the visibility watermark
A transaction SHALL read at the visibility watermark W when it begins: every commit acknowledged before it began SHALL
be visible to it, and no commit SHALL become visible to it later. A read-only transaction SHALL never be validated and
SHALL never abort for a conflict. A request naming a read version above W SHALL be refused with an error naming the
version and W, never held waiting for the watermark (decided 2026-10-27): every version a client holds came from the
server at or below W, and a commit is acknowledged only once W passes it.

#### Scenario: A read above the watermark is refused
- **WHEN** a request names a read version above the visibility watermark
- **THEN** it is refused at once with an error naming the version and the watermark, and the worker serves the next
  request

#### Scenario: A transaction sees every commit acknowledged before it began
- **WHEN** a commit setting `<a> <name> "A"` is acknowledged and a transaction then begins and reads `name` of `a`
- **THEN** it reads `"A"`

#### Scenario: A read-only transaction does not abort
- **WHEN** a read-only transaction reads `name` of `a`, another transaction then commits a new `name` of `a`, and the
  read-only transaction ends
- **THEN** it ends without an error, having read the value from before the commit

### Requirement: Transactions are strictly serializable
Every history of committed transactions SHALL be equivalent to a serial order in which a transaction acknowledged
before another began comes first.

#### Scenario: Write skew is refused
- **WHEN** two transactions each read `on-call` of `alice` and `bob`, both `true`, and each sets a different one to
  `false`, concurrently
- **THEN** one of them commits and the other aborts with a conflict

### Requirement: A transaction reads its own writes
Within one transaction, a read SHALL see the writes the transaction made before it, on top of its snapshot; no other
transaction SHALL see them before the transaction is acknowledged.

#### Scenario: A read after a write sees it
- **WHEN** a transaction inserts `<a> <knows> <b>` and then reads the objects of `knows` of `a`
- **THEN** the read holds `b`

### Requirement: What a transaction's reads record and what conflicts with them
A read-write transaction SHALL record: the key of each list it read whole; the list key and object of each statement
it tested; each key range it scanned; each dictionary key it found absent; and its namespace's catalog range. It SHALL
abort with a conflict when a transaction committed after its read version wrote, respectively: the list; that object in
that list; a key in the range; that dictionary entry; or the catalog. A commit that changed a list by more objects
than the commit step's bound SHALL be recorded as writing the whole list, which can only add conflicts. Writes made
without a read (reverse, predicate-list, index and statistics deltas, and inserts' forward deltas) SHALL never conflict
with each other.

#### Scenario: Inserts of different objects into one list both commit
- **WHEN** two transactions concurrently insert `<a> <knows> <b>` and `<a> <knows> <c>`
- **THEN** both commit, and `a` knows `b` and `c`

#### Scenario: Inserts of the same new statement conflict
- **WHEN** two transactions concurrently insert `<a> <knows> <b>`, which is not held
- **THEN** one commits and the other aborts with a conflict, and its retry finds the statement held

#### Scenario: A scanned range conflicts with an insert into it
- **WHEN** a transaction scans the `int` index of `age` for values from 30 to 40 and writes `<x> <seen> true`, and a
  transaction inserting `<c> <age> 35` commits after its read version
- **THEN** the scanning transaction aborts with a conflict

#### Scenario: An insert outside a scanned range does not conflict
- **WHEN** a transaction scans the `int` index of `age` for values from 30 to 40 and writes `<x> <seen> true`, and a
  transaction inserting `<c> <age> 50` commits after its read version
- **THEN** the scanning transaction commits

### Requirement: Transactions have bounded lifetimes
A read-write transaction SHALL live at most 10 s and a read-only transaction at most 16 s by default, each a setting. A
transaction past its limit SHALL be aborted at its next request or at its commit with a retryable error naming the
limit, and SHALL write nothing.

#### Scenario: A read-write transaction past its limit aborts
- **WHEN** a read-write transaction begins, waits 11 s, and commits
- **THEN** the commit fails with a retryable error naming the 10 s limit, and nothing it wrote is held

### Requirement: Interactive transactions
A client SHALL begin a transaction, send any number of reads and writes in separate requests, and end it with a commit
or an abort. An aborted transaction SHALL write nothing.

#### Scenario: An aborted transaction writes nothing
- **WHEN** a transaction inserts `<a> <knows> <b>` in one request and aborts in the next
- **THEN** `<a> <knows> <b>` is not held

### Requirement: Commit-time operations
A transaction SHALL be able to ask the commit step to decide an operation in commit order, after its other checks:
insert-if-absent on a predicate the catalog declares unique, which returns the UID of the node holding the value,
created if none does; upsert-and-link, which does the same and, in the same step, writes the links it names, edges with
the node as their subject or object; compare-and-set on a single-valued property; and counter add, which reads nothing
and commutes. Insert-if-absent and upsert-and-link SHALL write the properties they carry for creation only when they
create the node. Their writes SHALL go through index maintenance as a changeset's do, and their links SHALL be blind
deltas that never conflict. A commit-time operation SHALL not abort for the race it decides. A compare-and-set whose
expected value is not held SHALL refuse its whole transaction, writing nothing, and report the value it found. An
operation's result is known only at commit and is returned in the reply, with whether the node was created: the
transaction's other writes SHALL not name it, and the links an upsert-and-link carries are the way to connect it.

#### Scenario: Concurrent insert-if-absent yields one node
- **WHEN** eight transactions concurrently insert-if-absent a node with `epi.id` `"x"`, declared unique
- **THEN** all eight commit, all receive the same UID, and one node holds `epi.id` `"x"`

#### Scenario: Concurrent upsert-and-link deduplicates and links atomically
- **WHEN** eight transactions each create a memory `m1` to `m8` and concurrently upsert-and-link the entity with
  `epi.id` `"x"`, declared unique, each linking its memory to it by `HAS_ENTITY`
- **THEN** all eight commit and receive the same UID, one node holds `epi.id` `"x"`, each memory holds `HAS_ENTITY` to
  that node, and the node's reverse list of `HAS_ENTITY` holds all eight memories

#### Scenario: Properties for creation are written once
- **WHEN** two transactions concurrently upsert-and-link `epi.id` `"y"`, one carrying the label `"first"` for creation
  and the other `"second"`
- **THEN** the node holds the label of the transaction committed first and not the other, and only that transaction's
  reply says the node was created

#### Scenario: An upsert-and-link in an aborted transaction writes nothing
- **WHEN** a transaction upsert-and-links `epi.id` `"z"` and aborts with a conflict on another of its reads
- **THEN** no node holds `epi.id` `"z"` and no link of the transaction is held

#### Scenario: A failed compare-and-set writes nothing
- **WHEN** a transaction inserts `<a> <seen> true` and compare-and-sets `status` of `a` from `"open"` to `"closed"` while
  `status` is `"done"`
- **THEN** the transaction is refused reporting `"done"`, `status` of `a` stays `"done"`, and `<a> <seen> true` is not
  held

#### Scenario: Counter adds commute
- **WHEN** a hundred transactions concurrently add 1 to the counter `next-id`
- **THEN** all commit and `next-id` has grown by 100

### Requirement: Exactly-once commits through sessions
A client SHALL register a session, which the log records, and SHALL send each commit with its session and a request
number one above the session's last. A retry of the last committed request SHALL return its stored reply without
committing again; a request number out of order SHALL be refused; a session SHALL have at most one commit in flight.
The session table SHALL be bounded, and a request of an evicted session SHALL be refused with an error saying so.

#### Scenario: A retried commit commits once
- **WHEN** a client commits request 7 of its session, loses the reply, and sends request 7 again
- **THEN** it receives the same reply and commit version, and the transaction's writes are held once

#### Scenario: An evicted session is told so
- **WHEN** a session is evicted from the full table and its client sends its next request
- **THEN** the request is refused with an error saying the session was evicted

### Requirement: Aborts name their reason
An abort SHALL carry one of: conflict, with the kind of read that conflicted; too old, when the transaction's read
version is below what validation still holds; past its limit; busy, when the commit queue is full; too large, when its
write set exceeds the maximum frame; refused, when the catalog refuses a write; operation failed, when a
compare-and-set did not hold, with the value found. Conflict, too old, past its limit and busy SHALL be marked
retryable; too large, refused and operation failed SHALL not.

#### Scenario: A full commit queue answers busy
- **WHEN** the commit queue is full and a transaction commits
- **THEN** the commit fails with a retryable busy error, and nothing it wrote is held

#### Scenario: A write set over the maximum frame is refused
- **WHEN** a transaction's write set grows past 1 MiB and it commits
- **THEN** the commit fails with a too-large error that is not marked retryable, and nothing it wrote is held
