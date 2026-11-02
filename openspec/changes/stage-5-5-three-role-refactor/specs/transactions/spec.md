## MODIFIED Requirements

### Requirement: Transactions read a snapshot at the visibility watermark
A read-write transaction SHALL read its partition's latest state at the partition's visibility watermark W when it binds
to the partition: every commit acknowledged in that partition before it bound SHALL be visible to it, and no commit
SHALL become visible to it later; a session token it carries SHALL make its commit carry an epoch at or above the
token's ("Commits answer a session token"). A read-only transaction SHALL read at the level its Begin names (quad-reads,
"A read's transaction names its level"), waiting, at most until its deadline, for the epochs its level and its token
require. A read-only transaction SHALL never be validated and SHALL never abort for a conflict. A read version a client
names, an `as of` version or one its Begin names, SHALL be checked against the watermark W of the partition answering,
and a version above that W SHALL be refused at once with an error naming the version and W, never held waiting: every
version a client holds from a partition came from that partition at or below its W, and a commit is acknowledged only
once its partition's W passes it. Reads at a level, which compute asks of the partitions on the client's behalf, SHALL
wait as quad-reads states and are not checked against W.

#### Scenario: A read above the watermark is refused
- **WHEN** a client's Begin names a read version above the visibility watermark of the partition answering it
- **THEN** it is refused at once with an error naming the version and the watermark, and the worker serves the next
  request

#### Scenario: A transaction sees every commit acknowledged before it began
- **WHEN** a commit setting `<a> <name> "A"` is acknowledged and a read-write transaction then begins and reads `name`
  of `a`
- **THEN** it reads `"A"`

#### Scenario: A snapshot read waits for its epoch
- **WHEN** a read-only transaction at snapshot level begins while epoch 20 is the latest close, and one partition it
  reads has closed only epoch 19
- **THEN** its read answers once that partition has closed epoch 20, at epoch 20

#### Scenario: A read-only transaction does not abort
- **WHEN** a read-only transaction reads `name` of `a`, another transaction then commits a new `name` of `a`, and the
  read-only transaction ends
- **THEN** it ends without an error, having read the value from before the commit

### Requirement: Transactions are strictly serializable
Every history of committed transactions within one partition SHALL be equivalent to a serial order in which a
transaction acknowledged before another began comes first. A read-write transaction SHALL read and write within one
partition in this stage (partitions, "A read-write transaction stays within one partition in this stage").

#### Scenario: Write skew is refused
- **WHEN** two transactions each read `on-call` of `alice` and `bob`, both `true`, and each sets a different one to
  `false`, concurrently, all four facts in one partition
- **THEN** one of them commits and the other aborts with a conflict

### Requirement: What a transaction's reads record and what conflicts with them
A read-write transaction SHALL record: the key of each list it read whole; the list key and object of each statement it
tested; each key range it scanned; and each term it found absent, recorded as no statement of that term existing in the
transaction's partition. It SHALL abort with a conflict when a transaction committed in its partition after its read
version wrote what the table below names for one of its reads, a written statement counting as its list touched and a
written range being a drop:

| Read | A key written whole | A statement written | A range written (a drop) |
|---|---|---|---|
| a list read whole | that key | a statement of that list | one holding the key |
| a statement tested | its list key | that list key and object | one holding the list key |
| a range scanned | a key inside it | a statement whose list key is inside it | one sharing a key with it |
| a term found absent | none | a statement of that term | one holding a key of that term |

The conflict SHALL name the first kind of read that conflicts, in the order list, statement, range, absent term,
catalog, and a conflict with a written range SHALL be named a drop after them. The catalog is read through the
metadata group's version in effect at the transaction's commit epoch, not through a key range: a transaction that
executed against another version conflicts with the catalog, as commit-rounds' "Catalog changes, drops and index
builds are ordered in rounds" states. A commit that changed a list by more
objects than the commit step's bound SHALL be recorded as writing the whole list, which can only add conflicts. Writes
made without a read (reverse, predicate-list, index and statistics deltas, and inserts' forward deltas) SHALL never
conflict with each other. The dictionary is naming state, and registrations create its entries outside transactions,
so creating a dictionary entry SHALL conflict with nothing.

#### Scenario: Inserts of different objects into one list both commit
- **WHEN** two transactions concurrently insert `<a> <knows> <b>` and `<a> <knows> <c>`
- **THEN** both commit, and `a` knows `b` and `c`

#### Scenario: Inserts of the same new statement conflict
- **WHEN** two transactions in `<a>`'s partition concurrently insert `<a> <knows> <b>`, which is not held, `<b>` new
  and homed in another partition
- **THEN** both registrations of `<b>` yield one ID and neither aborts; one transaction commits and the other aborts
  with a conflict, and its retry finds the statement held

#### Scenario: A scanned range conflicts with an insert into it
- **WHEN** a transaction scans the `int` index of `age` for values from 30 to 40 and writes `<x> <seen> true`, and a
  transaction inserting `<c> <age> 35` commits after its read version
- **THEN** the scanning transaction aborts with a conflict

#### Scenario: An insert outside a scanned range does not conflict
- **WHEN** a transaction scans the `int` index of `age` for values from 30 to 40 and writes `<x> <seen> true`, and a
  transaction inserting `<c> <age> 50` commits after its read version
- **THEN** the scanning transaction commits

#### Scenario: A term found absent conflicts with a statement of it
- **WHEN** a transaction reads `<new>`'s statements, finds none, and writes `<x> <seen> true`, and a transaction in the
  same partition writing `<new> <name> "N"` commits after its read version
- **THEN** the reading transaction aborts with a conflict naming an absent term

#### Scenario: Creating a dictionary entry conflicts with nothing
- **WHEN** a transaction in `<new>`'s home reads `<new>`'s statements, finds none, and writes `<x> <seen> true`, and a
  registration from another partition creates `<new>`'s dictionary entry there after the read version, with no
  statement of `<new>` committed in the home
- **THEN** the reading transaction commits

### Requirement: Interactive transactions
A client SHALL begin a transaction, send any number of reads and writes in separate requests, and end it with a commit
or an abort. An aborted transaction SHALL write nothing, though the registrations it issued stay, naming nothing that
exists. A read-write transaction SHALL be bound as partitions' "A read-write transaction stays within one partition in
this stage" states: to the partition owning the first subject it reads or writes, to a partition the frontend chooses in
turn when its first write names only new blank nodes, to the partition owning the claimed value's bucket when its first
operation is an insert-if-absent or an upsert-and-link, and otherwise to the partition owning the namespace's whole run.

#### Scenario: An aborted transaction writes nothing
- **WHEN** a transaction inserts `<a> <knows> <b>` in one request and aborts in the next
- **THEN** `<a> <knows> <b>` is not held

### Requirement: Exactly-once commits through sessions
A client SHALL register a session through the frontend, which draws its 128-bit identity from its injected random source
and records it with the registering principal in the metadata group, one command per session and none per commit. A
session SHALL belong to that principal: the frontend SHALL check every commit's principal against the registry before
routing it, and only that principal SHALL commit through the session or read its stored replies. A client SHALL send
each commit with its session and a request number above the session's last, from one counter per session, since the
client does not know which partition a commit reaches. Only the metadata group's registry SHALL evict sessions
(metadata-group, "The metadata group registers sessions"). Each partition SHALL keep its own session table, keyed by the
session's 128-bit ID as 16 big-endian bytes, holding an entry for every registered session that committed there,
recorded at its first commit there with the last request number committed there and its stored reply, whose IRIs are
text; it SHALL purge an entry only after the registry has evicted that session, which the sequencer delivers to every
partition with its next close, so a table holds at most `sessions_max` entries; a partition SHALL refuse a request
number below its last for the session as out of order, and numbers the session committed in other partitions leave gaps
it accepts. A retry of a partition's last committed request SHALL return its stored reply without committing again; a
retry of a request the partition does not hold, with no transaction open for it, SHALL be refused as not committed; a
retry the frontend cannot route SHALL be asked of every partition and SHALL answer the stored reply of the one holding
it, or not committed when none does. A session SHALL have at most one commit in flight. A request of a session the
registry has evicted SHALL be refused by the frontend's owner check with an error saying so, so a partition never takes
an evicted session for one committing there for the first time.

#### Scenario: A retried commit commits once
- **WHEN** a client commits request 7 of its session, loses the reply, and sends request 7 again
- **THEN** it receives the same reply and commit version, and the transaction's writes are held once

#### Scenario: A session commits through two partitions
- **WHEN** a session commits request 1 in partition 1, request 2 in partition 2 and request 3 in partition 1, then loses
  the reply to request 3 and sends it again on a new connection
- **THEN** all three commit once, partition 1 accepts request 3 after its last, 1, and the retry receives request 3's
  stored reply from partition 1

#### Scenario: An evicted session is told so
- **WHEN** a session is evicted from the full registry and its client sends its next request
- **THEN** the frontend refuses the request with an error saying the session was evicted, and no partition sees it

#### Scenario: A partition purges an entry only after the registry's eviction
- **WHEN** a session has committed in partition 1, and the registry evicts it
- **THEN** partition 1 keeps the session's entry until the close carrying the eviction is delivered to it, and then
  purges it

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
transaction reaches a second partition, naming both (partitions, "A read-write transaction stays within one partition
in this stage"). Busy SHALL also answer a commit when its partition's epoch is full and the sequencer is unreachable
(commit-rounds, "Commit versions are built from the epoch"). Conflict, too old, past its limit and busy SHALL be marked
retryable; too large, refused, operation failed, permission denied and cross-partition SHALL not.

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

### Requirement: Commit-time operations
A transaction SHALL be able to ask the commit step to decide an operation in commit order, after its other checks:
insert-if-absent on a predicate the catalog declares unique, which returns the node holding the value, created if none
does; upsert-and-link, which does the same and, in the same step, writes the links it names, edges with the node as
their subject or object; compare-and-set on a single-valued property; and counter add, which reads nothing and
commutes. Insert-if-absent and upsert-and-link SHALL claim the value at the partition owning its bucket
(system-identity, "The bucket comes from the namespace and the key"); when that is not the transaction's partition the
transaction SHALL abort with cross-partition in this stage, which stage 8 sequences, and with one partition the claim is
local. Insert-if-absent and upsert-and-link SHALL write the properties they carry for creation only when they create
the node, and their result SHALL name the node by its IRI, or by its skolem IRI when the node is a blank node, never by
its system ID. Their writes SHALL go through index maintenance as a changeset's do, and their links SHALL be blind
deltas that never conflict. A commit-time operation SHALL not abort for the race it decides. A compare-and-set whose
expected value is not held SHALL refuse its whole transaction, writing nothing, and report the value it found. An
operation's result is known only at commit and is returned in the reply, with whether the node was created: the
transaction's other writes SHALL not name it, and the links an upsert-and-link carries are the way to connect it. The
round's log SHALL record the transaction's writes with its operations' resulting writes, so a replay applies them and
never runs an operation again. A counter add's result in the reply SHALL be the counter's new value, computed in commit
order, and a compare-and-set's SHALL be zero; a counter holding a value that is no integer, or that the add would
overflow, refuses its transaction as a failed compare-and-set does.

#### Scenario: Concurrent insert-if-absent yields one node
- **WHEN** eight transactions concurrently insert-if-absent a node with `epi.id` `"x"`, declared unique, all in the
  partition owning the bucket of `"x"`
- **THEN** all eight commit, all receive the same IRI, and one node holds `epi.id` `"x"`

#### Scenario: Concurrent upsert-and-link deduplicates and links atomically
- **WHEN** eight transactions each create a memory `m1` to `m8` and concurrently upsert-and-link the entity with
  `epi.id` `"x"`, declared unique, each linking its memory to it by `HAS_ENTITY`, all in the partition owning the
  bucket of `"x"`
- **THEN** all eight commit and receive the same IRI, one node holds `epi.id` `"x"`, each memory holds `HAS_ENTITY` to
  that node, and a reverse read of `HAS_ENTITY` for the node returns all eight memories

#### Scenario: Properties for creation are written once
- **WHEN** two transactions concurrently upsert-and-link `epi.id` `"y"`, one carrying the label `"first"` for creation
  and the other `"second"`, both in the partition owning the bucket of `"y"`
- **THEN** the node holds the label of the transaction committed first and not the other, and only that transaction's
  reply says the node was created

#### Scenario: An upsert-and-link in an aborted transaction writes nothing
- **WHEN** a transaction in the partition owning the bucket of `"z"` upsert-and-links `epi.id` `"z"` and aborts with
  a conflict on another of its reads
- **THEN** no node holds `epi.id` `"z"` and no link of the transaction is held

#### Scenario: A failed compare-and-set writes nothing
- **WHEN** a transaction inserts `<a> <seen> true` and compare-and-sets `status` of `a` from `"open"` to `"closed"` while
  `status` is `"done"`
- **THEN** the transaction is refused reporting `"done"`, `status` of `a` stays `"done"`, and `<a> <seen> true` is not
  held

#### Scenario: Counter adds commute
- **WHEN** a hundred transactions concurrently add 1 to the counter `next-id`
- **THEN** all commit and `next-id` has grown by 100

#### Scenario: A counter add returns the counter's new value
- **WHEN** the counter `next-id` holds 5, and two transactions each add 1 to it
- **THEN** both commit, the one committed first receives 6 and the other 7, and `next-id` holds 7

#### Scenario: A claim owned by another partition is cross-partition
- **WHEN** `epi.id` is declared unique, partition 2 owns the bucket of `"x"`, and a transaction bound to partition 1
  upsert-and-links the entity with `epi.id` `"x"`
- **THEN** the transaction aborts with a cross-partition error naming partitions 1 and 2, not marked retryable, and
  nothing it wrote is held

#### Scenario: A created node is named by its skolem IRI
- **WHEN** a transaction in the partition owning the bucket of `"w"` insert-if-absents a node with `epi.id` `"w"`,
  declared unique, and no node holds it
- **THEN** the reply names the created node by its skolem IRI, says it was created, and holds no system ID

## ADDED Requirements

### Requirement: Commits answer a session token
A committed transaction's reply SHALL carry a session token naming its partition, its log position and the epoch of its
commit version; a later read or transaction carrying the token SHALL see that commit and every commit before it in that
partition. A transaction carrying a token of epoch `t` SHALL commit at epoch `t` or later, its partition waiting until
the close of `t − 1` has been delivered to it, so a causally later write never falls in an earlier snapshot.

#### Scenario: A token names its commit
- **WHEN** a transaction commits in partition 1 at position 40 with a version of epoch 12
- **THEN** its reply carries the token (partition 1, position 40, epoch 12)

#### Scenario: A read with a token sees the commit
- **WHEN** a client commits `<a> <knows> <b>` and reads `<a>`'s statements with the commit's token from another connection
- **THEN** the read returns `<a> <knows> <b>`

#### Scenario: A causally later commit is not stamped earlier
- **WHEN** a session commits in partition 1 at epoch 20, and its next transaction, carrying that token, commits in
  partition 2 while partition 2's latest delivered close is 17
- **THEN** partition 2 commits it only after close 19 is delivered, at epoch 20 or later
