## ADDED Requirements

### Requirement: An open read-write transaction holds the fold horizon at its read version
A partition's primary SHALL keep the fold horizon it logs at or below the read version of its oldest open read-write
transaction (tile-engine, "The fold horizon is logged by the primary and folded at by every replica"), so every version
such a transaction reads stays answerable until it ends, and the logged horizon SHALL be the only bound on which
versions a read may take (storage-lanes, "The logged fold horizon is the only retention"). The lifetime limit of a
read-write transaction ("Transactions have bounded lifetimes") SHALL therefore bound how long one transaction holds the
horizon back. A read-only transaction's read below the latest logged horizon of the partition answering SHALL be
refused with code 3 (TooOld), retryable.

#### Scenario: A long read-write transaction keeps its version answerable
- **WHEN** a read-write transaction binds at version `v` and stays open 9 s with `retention_ms` 30,000, while rounds are
  logged and tiles rewritten
- **THEN** every round logged meanwhile carries a horizon at or below `v`, and the transaction's last read answers at
  `v`

#### Scenario: A named version below the horizon is too old
- **WHEN** a read-only transaction's Begin names a read version below the latest horizon the partition answering logged
- **THEN** its read is refused with code 3, marked retryable

## MODIFIED Requirements

### Requirement: What a transaction's reads record and what conflicts with them
A read-write transaction SHALL record its reads in lane X as conflict keys of the logical conflict families of
`specification/binary_protocol/PROTOCOL.md` section 12.2, before any answer of them leaves its partition (storage-lanes,
"What lane X records is the protocol's rule"): each statement it tested, as the statement's conflict key (conflict
family 5, Statement); each mark it read, as its conflict key (conflict family 6, Mark) as a range holding that key
alone; each range it read, as the range of conflict
keys the read covers, a row a Frontier reaches as the range of that row's key, a claim it read, the aggregate a `@limit`
check read among them, as that claim's key alone, a family read whole as the family's whole range, a frame ending inside
a row as its family's range in that row's bucket group, a read of every predicate as its graph's whole range and of
every graph as its tenant's; and each term it found absent, recorded as no statement of that term existing in the
transaction's partition, a statement's record found absent as its statement's ID. It SHALL record no entry or record it
finds of storage class I and SHALL record every absence (storage-lanes, "Class I reads are stable and absences are
always recorded"). It SHALL abort with a conflict when a transaction committed in its partition after its read version
wrote what the table below names for one of its reads, what a commit writes being its record's conflict points, conflict
statements and conflict ranges (`PROTOCOL.md` section 12.2, sections 6 to 8), a conflict range being a drop's:

| Read | A conflict point written | A conflict statement written | A conflict range written (a drop) |
|---|---|---|---|
| a statement tested | the statement's conflict key it recorded | none | one holding the key it recorded |
| a range read | a key inside it | a statement whose row key is inside it | one sharing a key with it |
| a term found absent | none | a statement of that term, or one whose statement ID it is | one holding a key of that term |

A transaction whose own conflict points or statements lie inside a drop's conflict range committed after its read
version SHALL abort with a conflict as well. The conflict SHALL name the first kind of read that conflicts, in the order
statement, absent term, range, catalog, and a conflict with a drop's range SHALL be named a drop after them
(`PROTOCOL.md` section 9.4, conflict kinds 2 to 6). The catalog is read through the metadata group's version in effect
at the transaction's commit epoch, not through a range: a transaction that executed against another version conflicts
with the catalog, as commit-rounds' "Catalog changes, drops and index builds are ordered in rounds" states. A drop's
range aside, a write SHALL conflict only with a read another transaction recorded, so writes made without a read never
conflict with each other. The dictionary is naming state, and registrations create its entries outside transactions, so
creating a dictionary entry SHALL conflict with nothing.

#### Scenario: Inserts of different objects into one row both commit
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

#### Scenario: A family read whole conflicts with a new row
- **WHEN** a transaction reads the `knows` family of graph `g` whole and writes `<x> <seen> true`, and a transaction
  inserting `<z> <knows> <y>`, `<z>` holding no `knows` before, commits after its read version
- **THEN** the reading transaction aborts with a conflict of kind Range

#### Scenario: A term found absent conflicts with a statement of it
- **WHEN** a transaction reads `<new>`'s statements, finds none, and writes `<x> <seen> true`, and a transaction in the
  same partition writing `<new> <name> "N"` commits after its read version
- **THEN** the reading transaction aborts with a conflict naming an absent term

#### Scenario: Creating a dictionary entry conflicts with nothing
- **WHEN** a transaction in `<new>`'s home reads `<new>`'s statements, finds none, and writes `<x> <seen> true`, and a
  registration from another partition creates `<new>`'s dictionary entry there after the read version, with no
  statement of `<new>` committed in the home
- **THEN** the reading transaction commits

#### Scenario: An absent term is named before the range that read it
- **WHEN** a transaction reads `<new> <p> ?o` through compute, finding nothing, so it records both the term's absence
  and the row's range, and a transaction writing `<new> <p> "x"` commits after its read version
- **THEN** the reading transaction aborts with a conflict of kind AbsentTerm

#### Scenario: A write into a dropped range conflicts
- **WHEN** a transaction writes `<a> <p> "x"` without reading `p`, and a Drop of `p` commits after its read version and
  before its commit
- **THEN** the transaction aborts with a conflict of kind Drop, and nothing of `p` is held

### Requirement: Aborts name their reason
An abort SHALL carry one of: conflict, with the kind of read that conflicted; too old, when the transaction's read
version is below what validation still holds or below the latest fold horizon its partition logged (storage-lanes, "The
logged fold horizon is the only retention"); past its limit; busy, when the commit queue is full, or when its
partition's store admits no commit until its deadline, `detail` 1 for delta admission and 2 for grid admission
(partitions, "A partition refuses commits its store cannot admit"); too large, when its write set exceeds the Welcome's
largest write set, as sent or as its commit-time operations leave it, the latter refused when the transaction is decided
and taking no commit version, or when its Commit reply's frame would pass 524,288 bytes, the floor of `max_frame`, since
its stored reply answers a resend on any connection, refused the same way, code 6, `detail` the bound; refused, when the
catalog refuses a write or a term breaks the server's rules; operation failed, when a compare-and-set did not hold, with
the value found; permission denied, when its principal may not do what a request asks; cross-partition, when the
transaction reaches a second partition, naming both (partitions, "A read-write transaction stays within one partition in
this stage"). A statement named by its reifier, deleted or with a property set or removed, SHALL be checked against the
subject its reifier resolves to, once the write's execution resolves it, and SHALL reach the partition owning that
subject's bucket. Busy SHALL also answer a commit when its partition's epoch is full and the sequencer is unreachable
(commit-rounds, "Commit versions are built from the epoch"). Conflict, too old, past its limit and busy SHALL be marked
retryable; too large, refused, operation failed, permission denied and cross-partition SHALL not.

#### Scenario: A full commit queue answers busy
- **WHEN** the commit queue is full and a transaction commits
- **THEN** the commit fails with a retryable busy error, and nothing it wrote is held

#### Scenario: A full delta answers busy with its detail
- **WHEN** a partition's delta stands at its watermark until a transaction's deadline passes, and the transaction
  commits
- **THEN** the commit fails with code 5, `detail` 1, marked retryable, and nothing it wrote is held

#### Scenario: A write set over the Welcome's bound is refused
- **WHEN** a transaction's write set grows past the Welcome's `max_write_set` and it commits
- **THEN** the commit fails with a too-large error that is not marked retryable, its `detail` the bound, and nothing it
  wrote is held

#### Scenario: A transaction reaching a second partition is not retryable
- **WHEN** a transaction bound to partition 1 writes a statement whose subject's bucket partition 2 owns
- **THEN** it aborts with a cross-partition error naming partitions 1 and 2, not marked retryable, and nothing it wrote
  is held

#### Scenario: A statement named by its reifier is checked at its subject
- **WHEN** a transaction bound to partition 1 sets a property of a statement named by its reifier, the statement's
  subject's bucket owned by partition 2
- **THEN** it aborts with a cross-partition error naming partitions 1 and 2, not marked retryable, and nothing it wrote
  is held

#### Scenario: A Commit reply past the floor of `max_frame` is refused
- **WHEN** a transaction's commit-time operations leave a Commit reply whose frame would pass 524,288 bytes
- **THEN** the commit is refused when it is decided, with code 6, `detail` 524,288, taking no commit version and writing
  nothing

### Requirement: Exactly-once commits through sessions
A client SHALL register a session through the frontend, which draws its 128-bit identity from its injected random source
and records it with the registering principal in the metadata group, one command per session and none per commit. A
session SHALL belong to that principal: the frontend SHALL check every commit's principal against the registry before
routing it, and only that principal SHALL commit through the session or read its stored replies. A client SHALL send
each commit with its session and a request number above the session's last, from one counter per session, since the
client does not know which partition a commit reaches. Only the metadata group's registry SHALL evict sessions
(metadata-group, "The metadata group registers sessions"). Each partition SHALL keep its own session table, keyed by the
session's 128-bit ID as 16 big-endian bytes alone, holding an entry for every registered session that committed there,
recorded at its first commit there with the last request number committed there and its stored reply, whose IRIs are
text (partitions, "The session table is a record run keyed by the session"); it SHALL delete an entry only at the round
that logs the registry's eviction of that session, the first round it logs after the close carrying the eviction is
delivered to it (commit-rounds, "A round logs the sessions evicted at it"), so a table holds at most `sessions_max`
entries; a partition SHALL refuse a request number below its last for the session as out of order, and numbers the
session committed in other partitions leave gaps it accepts. A Commit on an open transaction numbered at or below the
partition's last for the session SHALL be refused with code 13 (RequestOutOfOrder), `detail` its last, aborting the
transaction, so a reused number never reads another transaction's reply. A retry, a Recall or a one-frame or Drop
resend and never a Commit on an open transaction's stream, of a partition's last committed request SHALL return its
stored reply without committing again; a Commit's retry of a request the partition does not hold, with no transaction
open for it, SHALL be refused as not committed; a Commit's retry the frontend cannot route SHALL be asked of every
partition, and the frontend SHALL answer, in this order: a stored reply any partition gave; code 14 when any answered
14; code 17 when any answered 17 or a refusal not named here, which is a partition that could not be asked, or its role
connection closed or could not be opened before it answered; code 13 when any answered 13, `detail` that partition's
last; and, every partition having answered 18, code 12 when the registry has evicted the session meanwhile, else code
18. A resent Drop or one-frame write SHALL go as its first did and SHALL be decided by the partition's table: the stored
reply for its last request, code 14 for a request in its commit step, code 13 below its last, and otherwise it commits.
A request answered code 17 SHALL be in doubt, since the partition may hold it: the client keeps its request number and
resolves it by the resend before the session commits again. A new request refused with code 13 SHALL move the client's
counter above `detail`. A session SHALL have at most one commit in flight. A request of a session the registry has
evicted SHALL be refused by the frontend's owner check with an error saying so, so a partition never takes an evicted
session for one committing there for the first time. A full registry SHALL evict its oldest session with no Commit,
Drop or one-frame write in flight, which is from the owner check until the request ends before reaching a partition, its
partition answers it, or its role connection closes, whether or not its client stays connected, so a session that
passed the owner check is never evicted before its request reaches its partition; a Recall holds no session from
eviction. A Register finding no session the full registry may evict SHALL be refused with code 14, evicting nothing.

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

#### Scenario: A session with a commit in flight is not evicted
- **WHEN** the registry holds `sessions_max` sessions, the oldest has a Commit at its partition unanswered, its client's
  connection closes, and another session is registered
- **THEN** the registry evicts the next oldest session instead, and the oldest only once its partition has answered the
  Commit

#### Scenario: A full registry of commits in flight refuses a Register as busy
- **WHEN** the registry holds `sessions_max` sessions, each with a commit in flight, and a client registers another
- **THEN** the Register is refused with code 14, nothing is evicted, and a Register after a commit is answered is served

#### Scenario: A Cancel after a commit reaches its partition holds its session from eviction
- **WHEN** the registry holds `sessions_max` sessions, one with a Commit and one with a one-frame write at its partition
  unanswered, their client cancels both streams, and registers another session
- **THEN** the Register is refused with code 14, evicting neither, and a Register once the partition has answered both
  is served

#### Scenario: A partition deletes an entry at the round logging its eviction
- **WHEN** a session has committed in partition 1, and the registry evicts it
- **THEN** partition 1 keeps the session's entry until the close carrying the eviction is delivered to it, and the first
  round it logs after that close deletes the entry

#### Scenario: A retry of a request never committed is told so
- **WHEN** a client sends request 8 of its session again, with no transaction open for it, and the session's last
  committed request is 7
- **THEN** the request is refused as not committed, `detail` 8, and nothing is committed

#### Scenario: A session belongs to its principal
- **WHEN** principal `alice` registers a session, and a connection authenticated as `bob` commits through it in a
  partition where `alice` has not yet committed
- **THEN** the commit is refused with code 21, `detail` 2, nothing is committed, and `alice`'s next request through the
  session is served

#### Scenario: A reused number on an open transaction is refused
- **WHEN** a session's last request in partition 1 is 7, and a new transaction bound to partition 1 sends its Commit as
  request 7
- **THEN** the Commit is refused with code 13, `detail` 7, the transaction aborts, nothing it wrote is held, and request
  7's stored reply is not returned

#### Scenario: A commit answered code 17 is resolved by its resend
- **WHEN** a Commit through a session is answered code 17, and the partition had committed it
- **THEN** the client keeps the request number, sends the Commit again on a new stream, receives the stored reply and
  only then moves its counter

#### Scenario: A Recall reaching no answer stays in doubt
- **WHEN** a retried Commit is recalled over two partitions, partition 1 answers 18 and the role connection to partition
  2 closes under the Recall
- **THEN** the frontend answers code 17, and the client resends after a delay

#### Scenario: Any other refusal of a Recall stays in doubt
- **WHEN** a retried Commit is recalled over two partitions, partition 1 answers 18 and partition 2's address refuses
  the Recall with Error 1, serving another partition
- **THEN** the frontend answers code 17, and the client resends after a delay

#### Scenario: A stored reply outranks every other Recall answer
- **WHEN** a retried Commit is recalled over three partitions, one answers the stored reply, one 14, and one's
  connection closes
- **THEN** the client receives the stored reply

#### Scenario: A retry of a session evicted meanwhile is unknown
- **WHEN** a retried Commit is recalled, every partition answers 18, and the registry has evicted the session meanwhile
- **THEN** the frontend answers code 12, and the client reports the outcome as unknown

#### Scenario: A one-frame write's resend is decided by the table
- **WHEN** a DirectWrite through a session is answered code 17, the partition had not committed it, and the client sends
  it again
- **THEN** the frontend relays it, not recalls it, the partition commits it, and the reply carries its commit version
