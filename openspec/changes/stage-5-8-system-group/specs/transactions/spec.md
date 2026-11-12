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
code 13 when any answered 13, `detail` that partition's last; and, every partition having answered 18, code 12 when the
registry has evicted the session meanwhile, else code 18. A resent Drop or one-frame write SHALL go as its first did and
SHALL be decided by the partition's table: the stored reply for its last request, code 14 for a request in its commit
step, code 13 below its last, and otherwise it commits. A request answered code 17 SHALL be in doubt, since the
partition may hold it: the client keeps its request number and resolves it by the resend before the session commits
again. A new request refused with code 13 SHALL move the client's counter above `detail`. A session SHALL have at most
one commit in flight. A request of a session the registry has evicted SHALL be refused at the owner check, the frontend
answering the client with the code the Check gave, so a partition never takes an evicted session for one committing
there for the first time. A full registry SHALL evict its oldest session with no Commit, Drop or one-frame write in
flight, which is from the owner check until the request ends before reaching a partition, its partition answers it, its role connection closes, or, the connection that asked the check having closed, twice `transaction_ms` has passed (system-group, "A mark outlives its connection by twice the transaction limit"), whether or not its client stays connected, so a session that passed the owner check is never evicted
before its request reaches its partition; a Recall holds no session from eviction. A Register finding no session the
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

#### Scenario: A one-frame write's resend is decided by the table
- **WHEN** a DirectWrite through a session is answered code 17, the partition had not committed it, and the client sends
  it again
- **THEN** the frontend relays it, not recalls it, the partition commits it, and the reply carries its commit version
