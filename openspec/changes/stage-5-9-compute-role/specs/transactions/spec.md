## MODIFIED Requirements

### Requirement: Interactive transactions
A client SHALL begin a transaction, send any number of reads and writes in separate requests, and end it with a commit
or an abort. An aborted transaction SHALL write nothing, though the registrations it issued stay, naming nothing that
exists. A read-write transaction SHALL be bound as partitions' "A read-write transaction stays within one partition in
this stage" states: to the partition owning the first subject it reads or writes, to the partition owning the claimed
value's bucket when its first operation claims a value and names no subject but new blank nodes, to a partition compute
chooses in turn when its first write names only new blank nodes, and otherwise to the partition owning the tenant's
whole run. A tenant SHALL bind per Begin: every request of the transaction acts in the tenant its Begin names.

#### Scenario: An aborted transaction writes nothing
- **WHEN** a transaction inserts `<a> <knows> <b>` in one request and aborts in the next
- **THEN** `<a> <knows> <b>` is not held

### Requirement: Exactly-once commits through sessions
A client SHALL register a session through compute, which draws its 128-bit identity from its injected random source and
registers it with the registering principal in the metadata group by a Session command (system-group, "The registry is
asked by Session, Check and Ended"), one command per session and none per commit. A session SHALL belong to that
principal: compute SHALL ask the system group, by Check, to check every commit's principal against the registry before
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
transaction's reply. A retry, a Recall or a one-frame or Drop resend and never a Commit on an open transaction's stream,
of a partition's last committed request SHALL return its stored reply without committing again; a Commit's retry of a
request the partition does not hold, with no transaction open for it, SHALL be refused as not committed; a Commit's
retry compute cannot route SHALL be asked of every partition, and compute SHALL answer, in this order: a stored reply
any partition gave; code 14 when any answered 14; code 17 when any answered 17 or a refusal not named here, which is a
partition that could not be asked, or its role connection closed or could not be opened before it answered; code 13 when
any answered 13, `detail` that partition's last; and, every partition having answered 18, code 12 when the registry has
evicted the session meanwhile, else code 18. A resent Drop or one-frame write SHALL go as its first did and SHALL be
decided by the partition's table: the stored reply for its last request, code 14 for a request in its commit step, code
13 below its last, and otherwise it commits. A request answered code 17 SHALL be in doubt, since the partition may hold
it: the client keeps its request number and resolves it by the resend before the session commits again. A new request
refused with code 13 SHALL move the client's counter above `detail`. A session SHALL have at most one commit in flight.
A request of a session the registry has evicted SHALL be refused at the owner check, compute answering the client with
the code the Check gave, so a partition never takes an evicted session for one committing there for the first time. A
full registry SHALL evict its oldest session with no Commit, Drop or one-frame write in flight, which is from the owner
check until the request ends before reaching a partition, its partition answers it, its role connection closes, or, the
connection that asked the check having closed, `transaction_ms` has passed (system-group, "A mark outlives its
connection by the transaction limit"), whether or not its client stays connected, so a session that passed the owner
check is never evicted before its request reaches its partition; a Recall holds no session from eviction. A Register
finding no session the full registry may evict SHALL be refused with code 14, evicting nothing.

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
- **THEN** compute refuses the request with an error saying the session was evicted, and no partition sees it

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
- **THEN** compute answers code 17, and the client resends after a delay

#### Scenario: Any other refusal of a Recall stays in doubt
- **WHEN** a retried Commit is recalled over two partitions, partition 1 answers 18 and partition 2's address refuses
  the Recall with Error 1, serving another partition
- **THEN** compute answers code 17, and the client resends after a delay

#### Scenario: A stored reply outranks every other Recall answer
- **WHEN** a retried Commit is recalled over three partitions, one answers the stored reply, one 14, and one's
  connection closes
- **THEN** the client receives the stored reply

#### Scenario: A retry of a session evicted meanwhile is unknown
- **WHEN** a retried Commit is recalled, every partition answers 18, and the registry has evicted the session meanwhile
- **THEN** compute answers code 12, and the client reports the outcome as unknown

#### Scenario: A one-frame write's resend is decided by the table
- **WHEN** a DirectWrite through a session is answered code 17, the partition had not committed it, and the client sends
  it again
- **THEN** compute relays it, not recalls it, the partition commits it, and the reply carries its commit version

### Requirement: A request declares its type
A client SHALL declare a request's type by the command that carries it: read-only, DirectWrite, ConditionalWrite or
read-write. A DirectWrite SHALL be one frame of unconditional changes. A ConditionalWrite SHALL be one frame of
commit-time operations, with any changes, its conditions decided in commit order and its outcome reported in the reply.
Each SHALL commit all it holds or none, with no Begin and no open transaction, and SHALL hold the guarantees of any
transaction: strict serializability, exactly-once through a session, and acknowledgement at the visibility watermark.
Because no read reaches its client, a conflict on what its execution tested SHALL be retried by the partition, a bounded
number of times, before the client sees it. A DirectWrite writing a predicate the catalog declares unique SHALL be
refused, naming the predicate and ConditionalWrite, so that a request named direct runs no condition its client did not
ask for. A declaration SHALL be refused in either.

#### Scenario: An insert commits in one request
- **WHEN** a client sends a DirectWrite inserting `<a> <knows> <b>` through its session
- **THEN** the reply carries the commit version and session token, `<a> <knows> <b>` is held, and the partition holds no
  open transaction for it at any point after the reply

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
- **THEN** compute refuses it with code 23 before any relay, and neither is held

#### Scenario: A declaration in a write is refused
- **WHEN** a DirectWrite holds a declaration of a predicate
- **THEN** it is refused with a protocol error, and the catalog is unchanged

#### Scenario: A ConditionalWrite claiming a value goes to its claim
- **WHEN** a ConditionalWrite's changes name only new blank nodes as subjects, its operation is an insert-if-absent of
  `epi.id` `"x"`, and partition 2 owns the bucket of `"x"`
- **THEN** compute relays it to partition 2, and its reply names the node

#### Scenario: A resent blank-node write reaches one session table
- **WHEN** a DirectWrite through a session names only new blank nodes as subjects, its reply is lost, and the client
  sends it again
- **THEN** both go to the partition at the position XXH3-64 of its `session` and `request` bytes gives among the
  partitions holding the tenant's run, and the second is answered the stored reply

### Requirement: Commits answer a session token
A committed transaction's reply through a session SHALL carry a session token naming its partition, its log position and
the epoch of its commit version; a later read or transaction carrying the token SHALL see that commit and every commit
before it in that partition. A transaction carrying a token of epoch `t` SHALL commit at epoch `t` or later, its
partition waiting until the close of `t − 1` has been delivered to it, so a causally later write never falls in an
earlier snapshot. A reply through no session SHALL carry the zero token, a declaration's and a Drop's among them. A
transaction naming a read version SHALL read at that version whatever token it carries, and the token gives it nothing.
Compute SHALL refuse a Begin, DirectWrite or ConditionalWrite whose token's epoch is above the sequencer's open epoch
with Error 1 (Protocol) on its stream, the message naming the epoch, since no commit holds such a token. The Rust client
SHALL keep, per session, the token of the highest epoch among the session's commits, one-frame writes and resolved
resends included, and SHALL send it with every Begin it makes for that session and in every DirectWrite and
ConditionalWrite through it; a zero token replaces nothing.

#### Scenario: A token names its commit
- **WHEN** a transaction commits in partition 1 at position 40 with a version of epoch 12
- **THEN** its reply carries the token (partition 1, position 40, epoch 12)

#### Scenario: A read with a token sees the commit
- **WHEN** a client commits `<a> <knows> <b>` and reads `<a>`'s statements with the commit's token from another
  connection
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
