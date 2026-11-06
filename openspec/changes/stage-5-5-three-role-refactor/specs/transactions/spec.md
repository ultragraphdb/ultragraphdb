## MODIFIED Requirements

### Requirement: Transactions read a snapshot at the visibility watermark
A read-write transaction SHALL read its partition's latest state at the partition's visibility watermark W when it binds
to the partition: every commit acknowledged in that partition before it bound SHALL be visible to it, and no commit
SHALL become visible to it later; a session token it carries SHALL make its commit carry an epoch at or above the
token's ("Commits answer a session token"). A read-only transaction SHALL read at the level its Begin names (quad-reads,
"A read's transaction names its level"), waiting, at most until its deadline, for the epochs its level and its token
require. A read-only transaction SHALL never be validated and SHALL never abort for a conflict. A read version a client
names, an `as of` version or one its Begin names, SHALL be checked against the watermark W of the partition answering.
A version within an epoch that partition has closed SHALL read what it applied at or below the version, since an idle
partition's W stays below a version another partition reached. Any other version above that W SHALL be refused at once with
an error naming the version and W, never held waiting. Reads at a level, which compute asks of the partitions on the client's behalf, SHALL
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
| a range scanned | a key inside it, a statement's record included | a statement whose list key is inside it | one sharing a key with it |
| a term found absent | none | a statement of that term | one holding a key of that term |

The conflict SHALL name the first kind of read that conflicts, in the order list, statement, absent term, range,
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

#### Scenario: An absent term is named before the range that read it
- **WHEN** a transaction reads `<new> <p> ?o` through compute, finding nothing, so it records both the term's absence
  and the list's range, and a transaction writing `<new> <p> "x"` commits after its read version
- **THEN** the reading transaction aborts with a conflict of kind AbsentTerm

### Requirement: Interactive transactions
A client SHALL begin a transaction, send any number of reads and writes in separate requests, and end it with a commit
or an abort. An aborted transaction SHALL write nothing, though the registrations it issued stay, naming nothing that
exists. A read-write transaction SHALL be bound as partitions' "A read-write transaction stays within one partition in
this stage" states: to the partition owning the first subject it reads or writes, to the partition owning the claimed value's
bucket when its first operation claims a value and names no subject but new blank nodes, to a partition the frontend chooses in
turn when its first write names only new blank nodes, and otherwise to the partition owning the tenant's whole run. A tenant
SHALL bind per Begin: every request of the transaction acts in the tenant its Begin names.

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
it accepts. A Commit on an open transaction numbered at or below the partition's last for the session SHALL be refused
with code 13 (RequestOutOfOrder), `detail` its last, aborting the transaction, so a reused number never reads another
transaction's reply. A retry, a Recall or a one-frame or Drop resend and never a Commit on an open transaction's stream, of a
partition's last committed request SHALL return its stored reply without committing again; a Commit's retry of a request
the partition does not hold, with no transaction open for it, SHALL be refused as not committed; a Commit's retry the
frontend cannot route SHALL be asked of every partition, and the frontend SHALL answer, in this order: a stored reply any
partition gave; code 14 when any answered 14; code 17 when any answered 17 or a refusal not named here, which is a partition
that could not be asked, or its role connection closed or could not be opened before it answered; code 13 when any answered 13, `detail` that partition's last; and, every partition having
answered 18, code 12 when the registry has evicted the session meanwhile, else code 18. A resent Drop or one-frame write SHALL
go as its first did and SHALL be decided by the partition's table: the stored reply for its last request, code 14 for a request
in its commit step, code 13 below its last, and otherwise it commits. A request answered code 17 SHALL be in doubt, since the
partition may hold it: the client keeps its request number and resolves it by the resend before the session commits again. A
new request refused with code 13 SHALL move the client's counter above `detail`. A session SHALL have at most one commit in flight. A request of a session the
registry has evicted SHALL be refused by the frontend's owner check with an error saying so, so a partition never takes
an evicted session for one committing there for the first time. A full registry SHALL evict its oldest session with no
Commit, Drop or one-frame write in flight, which is from the owner check until the request ends before reaching a
partition, its partition answers it, or its role connection closes, whether or not its client stays connected, so a
session that passed the owner check is never evicted before its request reaches its partition; a Recall holds no session
from eviction. A Register finding no session the full registry may evict SHALL be refused with code 14, evicting nothing.

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
- **THEN** the Register is refused with code 14, evicting neither, and a Register once the partition has answered both is
  served

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
- **WHEN** a retried Commit is recalled over two partitions, partition 1 answers 18 and partition 2's address refuses the
  Recall with Error 1, serving another partition
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

### Requirement: Aborts name their reason
An abort SHALL carry one of: conflict, with the kind of read that conflicted; too old, when the transaction's read
version is below what validation still holds; past its limit; busy, when the commit queue is full; too large, when its
write set exceeds the Welcome's largest write set, as sent or as its commit-time operations leave it, the latter
refused when the transaction is decided and taking no commit version, or when its Commit reply's frame
would pass 524,288 bytes, the floor of `max_frame`, since its stored reply answers a resend on any connection, refused the same
way, code 6, `detail` the bound; refused, when the catalog
refuses a write or a term breaks the server's rules; operation failed, when a compare-and-set did not hold, with the
value found; permission denied, when its principal may not do what a request asks; cross-partition, when the
transaction reaches a second partition, naming both (partitions, "A read-write transaction stays within one partition
in this stage"). A statement named by its reifier, deleted or with a property set or removed, SHALL be checked against
the subject its reifier resolves to, once the write's execution resolves it, and SHALL reach the partition owning that
subject's bucket (amended 2026-11-03). Busy SHALL also answer a commit when its partition's epoch is full and the sequencer is unreachable
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

#### Scenario: A statement named by its reifier is checked at its subject
- **WHEN** a transaction bound to partition 1 sets a property of a statement named by its reifier, the statement's
  subject's bucket owned by partition 2
- **THEN** it aborts with a cross-partition error naming partitions 1 and 2, not marked retryable, and nothing it wrote
  is held

#### Scenario: A Commit reply past the floor of `max_frame` is refused
- **WHEN** a transaction's commit-time operations leave a Commit reply whose frame would pass 524,288 bytes
- **THEN** the commit is refused when it is decided, with code 6, `detail` 524,288, taking no commit version and writing
  nothing

### Requirement: Commit-time operations
A transaction SHALL be able to ask the commit step to decide an operation in commit order, after its other checks:
insert-if-absent on a predicate the catalog declares unique, which returns the node holding the value, created if none
does; upsert-and-link, which does the same and, in the same step, writes the links it names, edges with the node as
their subject or object; compare-and-set on a single-valued property; and counter add, which reads nothing and
commutes. Insert-if-absent and upsert-and-link SHALL claim the value at the partition owning its bucket
(system-identity, "The bucket comes from the tenant and the key"); when that is not the transaction's partition the
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

### Requirement: A request declares its type
A client SHALL declare a request's type by the command that carries it: read-only, DirectWrite, ConditionalWrite or read-write. A
DirectWrite SHALL be one frame of unconditional changes. A ConditionalWrite SHALL be one frame of commit-time operations, with any
changes, its conditions decided in commit order and its outcome reported in the reply. Each SHALL commit all it holds or none, with
no Begin and no open transaction, and SHALL hold the guarantees of any transaction: strict serializability, exactly-once through a
session, and acknowledgement at the visibility watermark. Because no read reaches its client, a conflict on what its execution
tested SHALL be retried by the partition, a bounded number of times, before the client sees it. A DirectWrite writing a predicate the
catalog declares unique SHALL be refused, naming the predicate and ConditionalWrite, so that a request named direct runs no condition
its client did not ask for. A declaration SHALL be refused in either.

#### Scenario: An insert commits in one request
- **WHEN** a client sends a DirectWrite inserting `<a> <knows> <b>` through its session
- **THEN** the reply carries the commit version and session token, `<a> <knows> <b>` is held, and the partition holds no open
  transaction for it at any point after the reply

#### Scenario: A DirectWrite on a unique predicate is refused
- **WHEN** a DirectWrite holds a valid insert and an insert of the predicate `id`, which the catalog declares unique
- **THEN** it is refused with an error naming `id` and ConditionalWrite, and neither insert is held

#### Scenario: Concurrent identical inserts both succeed
- **WHEN** two DirectWrites insert `<a> <knows> <b>` concurrently
- **THEN** both are answered with success, whichever executed first, and the statement is held once

#### Scenario: An upsert decides in commit order
- **WHEN** two ConditionalWrites each insert-if-absent a node with `id` `"x"` concurrently
- **THEN** both name the same node, created once

#### Scenario: A failed condition is an answer
- **WHEN** a ConditionalWrite holds a compare-and-set whose expected value does not hold
- **THEN** it is refused with `OperationFailed` and the value found, is not retried, and nothing it holds is written

#### Scenario: A retried write commits once
- **WHEN** a client loses the reply to request 7 of its session, a DirectWrite, and sends it again
- **THEN** it receives the same reply and commit version, and the writes are held once

#### Scenario: A write reaching two partitions is refused
- **WHEN** a DirectWrite writes subjects of partitions 1 and 2
- **THEN** the frontend refuses it with code 23 before any relay, and neither is held

#### Scenario: A declaration in a write is refused
- **WHEN** a DirectWrite holds a declaration of a predicate
- **THEN** it is refused with a protocol error, and the catalog is unchanged

#### Scenario: A ConditionalWrite claiming a value goes to its claim
- **WHEN** a ConditionalWrite's changes name only new blank nodes as subjects, its operation is an insert-if-absent of
  `epi.id` `"x"`, and partition 2 owns the bucket of `"x"`
- **THEN** the frontend relays it to partition 2, and its reply names the node

#### Scenario: A resent blank-node write reaches one session table
- **WHEN** a DirectWrite through a session names only new blank nodes as subjects, its reply is lost, and the client
  sends it again
- **THEN** both go to the partition at the position XXH3-64 of its `session` and `request` bytes gives among the
  partitions holding the tenant's run, and the second is answered the stored reply

### Requirement: Commits answer a session token
A committed transaction's reply through a session SHALL carry a session token naming its partition, its log position and the
epoch of its commit version; a later read or transaction carrying the token SHALL see that commit and every commit before it in that
partition. A transaction carrying a token of epoch `t` SHALL commit at epoch `t` or later, its partition waiting until
the close of `t − 1` has been delivered to it, so a causally later write never falls in an earlier snapshot. A reply
through no session SHALL carry the zero token, a declaration's and a Drop's among them. A transaction naming a read version SHALL read at
that version whatever token it carries, and the token gives it nothing. The frontend SHALL refuse a Begin, DirectWrite or
ConditionalWrite whose token's epoch is above the sequencer's open epoch with Error 1 (Protocol) on its stream, the message
naming the epoch, since no commit holds such a token. The Rust client SHALL keep, per session, the token of the highest epoch
among the session's commits, one-frame writes and resolved resends included, and SHALL send it with every Begin it makes for
that session and in every DirectWrite and ConditionalWrite through it; a zero token replaces nothing.

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

#### Scenario: A reply through no session carries no token
- **WHEN** a transaction and a Drop each commit through no session, `session` all zero
- **THEN** each reply's `commit_version` is its version and its token is 24 zero bytes, and a receiver accepts it

#### Scenario: A named read version takes nothing from a token
- **WHEN** a read-only transaction at snapshot level names a read version in epoch 12, and carries a token of epoch 20
- **THEN** its reads answer at the named version, and none waits for epoch 20

#### Scenario: A token above the open epoch is refused
- **WHEN** the sequencer's open epoch is 30, and a Begin, a DirectWrite and a ConditionalWrite each carry a token of
  epoch 31
- **THEN** each is refused with Error 1 on its stream, the message naming epoch 31

#### Scenario: A one-frame write carries the session's token
- **WHEN** a client's session commits a DirectWrite whose reply's token is of epoch 12, resolves a resend whose stored
  reply's is of epoch 9, then sends a ConditionalWrite through the session
- **THEN** the ConditionalWrite carries the token of epoch 12

#### Scenario: A zero token replaces nothing
- **WHEN** a client's session holds a token of epoch 12, and a commit through no session answers the zero token
- **THEN** the session still sends the token of epoch 12

### Requirement: An ended transaction releases what it holds
A transaction ending without a commit SHALL release the dictionary entries it created or took while uncommitted, with
everything else its execution holds: at its abort, at a refusal ending it, at its lifetime limit, and when the commit
step aborts it. A released entry SHALL never be written to the store: its ID is never used again, and a transaction that
took the same ID while the first was open carries the entry in its own round's record, so the log alone resolves every
ID a committed statement names. A storage partition's host SHALL abort, on its own timer, each open transaction past its
lifetime limit ("Transactions have bounded lifetimes"), whether or not its client sends another request, and SHALL abort
a transaction at once when the connection carrying it closes. A client reconnecting SHALL not resume an open transaction:
the transaction is aborted, and the client runs it again, its commit made exactly-once by its session.

#### Scenario: A vanished client's transaction is aborted at its limit
- **WHEN** a read-write transaction writes `<new> <p> "x"`, creating `<new>`, and its client then sends nothing, its
  connection staying open
- **THEN** the host aborts the transaction once 10 s have passed, nothing it wrote is held, the dictionary holds no
  entry it created for `<new>`, and a later transaction writing `<new>` commits

#### Scenario: A closed connection aborts its transaction at once
- **WHEN** a read-write transaction creates `<new>` and the connection carrying it closes after 1 s
- **THEN** the host aborts the transaction then, not at its limit, and its entry for `<new>` is released

#### Scenario: A released entry never reaches the store
- **WHEN** transaction T1 creates `<gone>`, transaction T2 writes `<gone>` while T1 is open, T1 aborts and T2 commits,
  and the store is discarded and the log replayed into an empty store
- **THEN** T2's record carries `<gone>`'s entry, replay resolves `<gone>` to the ID T2 committed, and nothing T1 created
  is held
