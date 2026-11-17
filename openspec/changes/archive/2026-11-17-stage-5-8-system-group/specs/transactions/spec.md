## MODIFIED Requirements

### Requirement: Exactly-once commits through sessions
A client SHALL register a session through the frontend, which draws its 128-bit identity from its injected random source
and registers it with the registering principal in the metadata group by a Session command (system-group, "The registry
is asked by Session, Check and Ended"), one command per session and none per commit. A session SHALL belong to that
principal: the frontend SHALL ask the system group, by Check, to check every commit's principal against the registry
before routing it, and only that principal SHALL commit through the session or read its stored replies. A client SHALL
send each commit with its session and a request number above the session's last, from one counter per session, since the
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
retry the frontend cannot route SHALL be asked of every partition, and the frontend SHALL answer, in this order: a
stored reply any partition gave; code 14 when any answered 14; code 17 when any answered 17 or a refusal not named here,
which is a partition that could not be asked, or its role connection closed or could not be opened before it answered;
code 13 when any answered 13, `detail` that partition's last; and, every partition having answered 18, by the outcome of a second Check of kind 1: code 12 when the
registry has evicted the session meanwhile, code 21 when the session belongs to another principal, code 14 when the check answers 14, else code 18. A resent Drop or one-frame write SHALL go as its first did and
SHALL be decided by the partition's table: the stored reply for its last request, code 14 for a request in its commit
step, code 13 below its last, and otherwise it commits. A request answered code 17 SHALL be in doubt, since the
partition may hold it: the client keeps its request number and resolves it by the resend before the session commits
again. A new request refused with code 13 SHALL move the client's counter above `detail`. A session SHALL have at most
one commit in flight. A request of a session the registry has evicted SHALL be refused at the owner check, the frontend
answering the client with the code the Check gave, so a partition never takes an evicted session for one committing
there for the first time. A full registry SHALL evict its oldest session with no Commit, Drop or one-frame write in
flight, which is from the owner check until the request ends before reaching a partition, its partition answers it, its role connection closes, or, the connection that asked the check having closed, twice `transaction_ms` has passed (system-group, "A mark outlives its connection by twice the transaction limit"), whether or not its client stays connected, so a session that passed the owner check is never evicted
before its request reaches its partition, except by its principal's removal (metadata-group, "Principals live in the
metadata group"); a Recall holds no session from eviction. A Register finding no session the
full registry may evict SHALL be refused with code 14, evicting nothing.

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

#### Scenario: A retry checked in the quiet time stays undecided
- **WHEN** a retried Commit is recalled, every partition answers 18, and the second Check answers 14, the system group
  having begun to listen less than twice `transaction_ms` before
- **THEN** the frontend answers code 14, and the client resends after a delay

#### Scenario: A one-frame write's resend is decided by the table
- **WHEN** a DirectWrite through a session is answered code 17, the partition had not committed it, and the client sends
  it again
- **THEN** the frontend relays it, not recalls it, the partition commits it, and the reply carries its commit version

### Requirement: Commits answer a session token
A committed transaction's reply through a session SHALL carry a session token naming its partition, its log position and
the epoch of its commit version; a later read or transaction carrying the token SHALL see that commit and every commit
before it in that partition. A transaction carrying a token of epoch `t` SHALL commit at epoch `t` or later, its
partition waiting until the close of `t − 1` has been delivered to it, so a causally later write never falls in an
earlier snapshot. A reply through no session SHALL carry the zero token, a declaration's and a Drop's among them. A
transaction naming a read version SHALL read at that version whatever token it carries, and the token gives it nothing.
A Begin naming a token whose epoch is above the endpoint core's open epoch SHALL be held until the core's stream of
closes delivers the close before the token's epoch, and a DirectWrite or ConditionalWrite naming one SHALL go to its
partition, which holds the commit until that close (`PROTOCOL.md` section 4.8; client-endpoint, "An endpoint core keeps
a view of what the closes announce"), since a partition may have received a close the core has not. The Rust client
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

#### Scenario: A one-frame write carries the session's token
- **WHEN** a client's session commits a DirectWrite whose reply's token is of epoch 12, resolves a resend whose stored
  reply's is of epoch 9, then sends a ConditionalWrite through the session
- **THEN** the ConditionalWrite carries the token of epoch 12

#### Scenario: A zero token replaces nothing
- **WHEN** a client's session holds a token of epoch 12, and a commit through no session answers the zero token
- **THEN** the session still sends the token of epoch 12
