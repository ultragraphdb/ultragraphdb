# client-endpoint Specification

## Purpose
The frontend's client endpoint: the binary protocol served to clients over TLS 1.3 or clear text, SCRAM authentication
against the metadata group's principals, one gate for every request naming a tenant, writes routed to their partitions
and reads spread over compute, the bounds and timers a connection runs under, and one error table, so every client
request enters the cluster at one place that authenticates, authorises and bounds it.
## Requirements
### Requirement: The frontend serves the binary protocol's clients
The frontend SHALL accept client connections at its configured address, run the handshake and version negotiation of
the wire protocol, and serve each connection on one core's runtime. It SHALL bound its connections and each
connection's streams, prepared statements and pending requests, answering past a bound with the wire's limit error.

#### Scenario: Connections past the bound are refused
- **WHEN** the frontend allows 2 connections and a third client connects
- **THEN** the third connection is refused with the limit error, and the first two keep working

### Requirement: The client endpoint speaks TLS 1.3 when configured
The frontend's client endpoint SHALL speak TLS 1.3 from the first byte when its `[endpoint]` table names
`tls_cert_file` and `tls_key_file`, through `ultragraphdb_transport_tls`' acceptor (transport-tls), and clear text only
when it sets `plaintext = true`; naming neither, or both a certificate and `plaintext = true`, SHALL stop the start
with code 78. The `dev` profile SHALL set `plaintext = true`. The client shells SHALL connect with TLS when their
configuration names a trust source, verifying the server's certificate for the host they dial before the handshake of
the wire protocol begins. Inside TLS, authentication SHALL be SCRAM-SHA-256 as on a clear-text connection.

#### Scenario: A TLS client reaches the endpoint
- **WHEN** the frontend holds the test CA's certificate for `localhost` and a client trusting the CA connects and
  authenticates
- **THEN** the TLS handshake, the wire handshake and SCRAM complete, and a write and a read succeed

#### Scenario: A clear-text client is refused by a TLS endpoint
- **WHEN** a client without TLS sends the wire protocol's first frame to an endpoint configured for TLS
- **THEN** the connection fails as a TLS handshake failure, and no frame of the wire protocol is read

#### Scenario: An endpoint with no TLS decision does not start
- **WHEN** the frontend starts with neither a certificate nor `plaintext = true` in its `[endpoint]` table
- **THEN** it exits with code 78 and an error naming `tls_cert_file`, `tls_key_file` and `plaintext`

### Requirement: Connections authenticate against the metadata group's principals
A connection SHALL authenticate with SCRAM-SHA-256 against a principal the metadata group holds before any data request,
and every request SHALL run with that principal's rights. The frontend SHALL hold no principal's verifier: for each
exchange it SHALL send the name in a Challenge and the AuthMessage and the client's proof in a Verify to the system
group, and pass on the salt, the iteration count and the ServerSignature it answers (system-group, "A client's proof is
checked in the system group"). A failed authentication SHALL be answered with the wire's authentication error without
saying whether the name or the proof was wrong. The frontend SHALL count each source's consecutive failed
authentications, in one of its runtimes for all of them, a source an IPv4 address or an IPv6 /64, a success clearing its
count: past `[endpoint] auth_failures_before_throttle` failures, 50 by default, each attempt of the source SHALL wait a
delay doubling from 100 ms to at most 30 s; past `auth_throttled_failures_before_ban` more, 20 by default, the source
SHALL be banned for `auth_ban_ms`, 24 hours by default, each of its connections closed as accepted, before TLS or any
frame, until the ban ends on its own. The counts SHALL be one for every endpoint core of a process, and each process
holding a client endpoint SHALL count for itself. The server SHALL answer an unknown name with a verifier derived from
the secret the metadata group holds (metadata-group, "Principals live in the metadata group"), so a name's salt tells no
prober whether it exists.

#### Scenario: A wrong password is refused without detail
- **WHEN** a client authenticates as `ann` with a wrong password, and as an unknown name
- **THEN** both are refused with the same authentication error

#### Scenario: The frontend never holds a stored key
- **WHEN** a client authenticates as `ann` and the frames the frontend receives from the system group are read
- **THEN** they hold `ann`'s salt, iteration count, principal and the exchange's ServerSignature, and neither her
  stored key nor her server key

#### Scenario: A data request before authentication is refused
- **WHEN** a client sends a Read before authenticating
- **THEN** the request is refused with the authentication error, and the connection stays open for authentication

#### Scenario: An unknown name answers as a known one does
- **WHEN** a client asks to authenticate as an unknown name, twice, across a restart of the frontend
- **THEN** both challenges carry the same salt and iteration count, and the exchange fails with the same authentication
  error as a wrong password

#### Scenario: A source past its failures is throttled, then banned
- **WHEN** one address fails to authenticate past the throttle's threshold, and then past the ban's
- **THEN** its attempts past the first threshold wait a growing delay, its connections past the second are closed as accepted,
  another address still authenticates, and once the ban's time has passed the address authenticates again

### Requirement: Requests are routed to the partitions and compute that serve them
The frontend SHALL convert a request's terms, find each subject's bucket and partition from its placement map, and send
a write to its partition's storage process and a read to compute; a reply SHALL reach the client on the request's
stream. The frontend SHALL check every subject a first request or a one-frame write names by an IRI or a skolem IRI against its
map, and SHALL refuse one whose subjects spread over partitions with code 23 before any Bind or relay. A partition's refusal naming
another owner SHALL make the frontend refresh its placement map and resend once; code 26 of `detail` 0 names no owner, and the frontend
SHALL answer it to the client. Until a transaction's first request is answered,
the frontend SHALL send none of its later requests to a partition or to compute, keeping them, and on code 26 SHALL send them after
the resend, in order; a first read SHALL count as answered, for this rule alone, at its first frame or its Error. When compute
answers a first read with code 23, the frontend SHALL cancel the Bind stream and refresh its map: if the refreshed map moves the
read's subject, it SHALL bind to the owner and resend the read once. Otherwise, and for any later read compute answers so, it SHALL
relay code 23 on the read's stream, drop the writes it holds, cancel the transaction's other reads still open, and answer with code
23, the first's `detail`, each of those reads, each read it holds, every later Read, Execute or Query naming the transaction and the
next request on the transaction's stream, which ends it.

#### Scenario: A write reaches its partition
- **WHEN** a client writes one statement per partition, each subject an IRI of that partition's buckets
- **THEN** the frontend sends each Write first to the partition owning its subject's bucket, and each commit's reply reaches
  the client

#### Scenario: A stale placement map is corrected
- **WHEN** a partition refuses a write naming its new owner
- **THEN** the frontend refreshes its map, resends the write to the owner once, and the client receives the owner's reply

#### Scenario: A home's code 26 reaches the client
- **WHEN** a partition answers a client's first Write with code 26 of `detail` 0, a home having refused its registration
- **THEN** the client receives code 26 on the transaction's stream, and the frontend binds no other partition

#### Scenario: Later requests wait for the first
- **WHEN** a client sends two Writes of a new transaction without waiting, and the partition refuses the first with code
  26
- **THEN** the second was sent to no partition; the frontend refreshes its map, binds to the owner, and sends the first
  and then the second, and the client sees no error

#### Scenario: A first read moved by a stale map is resent
- **WHEN** a transaction's first request is a read, compute answers code 23, and the refreshed map moves the read's
  subject to partition 2
- **THEN** the frontend cancels the Bind stream, binds to partition 2 and resends the read once, and the client receives
  its rows

#### Scenario: A first read the map does not move is refused
- **WHEN** a transaction's first request is a read, compute answers code 23, and the refreshed map leaves the read's
  subject where it was
- **THEN** the client's read is refused with code 23, and its next request on the transaction's stream too

#### Scenario: A read's code 23 answers the transaction's other reads
- **WHEN** reads A and C of a read-write transaction are open and a Write is held behind them, compute answers A with code
  23, and the client then sends read D
- **THEN** A, C and D are each answered code 23, the `detail` of A's, the Write is sent to no partition, and the next
  request on the transaction's stream is answered code 23 and ends it

#### Scenario: A first request spreading over partitions is refused before any Bind
- **WHEN** a transaction's first Write names subjects the frontend's map places in partitions 1 and 2
- **THEN** it is refused with code 23 on the transaction's stream, `detail` partitions 1 and 2, no partition receives a Bind,
  and the transaction ends

#### Scenario: A first read holds a later read only until its first frame
- **WHEN** a transaction's first read takes six frames at `initial_credit` 4, and after its first batch the client sends a
  second read and waits on it
- **THEN** the frontend sends the second read to compute on the first read's first frame, and the client receives its rows
  with the first read still open

### Requirement: Reads are spread over the compute processes
The frontend SHALL send each read to the next compute process of its members, round-robin per endpoint core. It SHALL
move to the next only when the connection cannot be opened, or closes before any frame of the read reached the client,
and with none left SHALL refuse the read with code 17 (Unavailable). Once a frame has reached the client the read SHALL
never be resent, and a closed connection SHALL answer it with code 17. A compute process sending no frame of a read for
`scan_stall_ms` past the later of the read's deadline and its last frame, while the read's stream holds credit, is hung:
the frontend SHALL cancel the ComputeRead and answer the read with code 17. The frontend SHALL read `scan_stall_ms` from
the `[storage]` table of its own settings.

#### Scenario: Reads alternate between two compute processes
- **WHEN** two compute processes have joined and a connection sends two reads
- **THEN** each process receives one

#### Scenario: A closed compute process is skipped
- **WHEN** the connection to one of two compute processes closes under a read before any frame of it reached the client
- **THEN** the frontend sends the read to the other, and the client receives its rows

#### Scenario: No compute process can be reached
- **WHEN** no connection to a compute process can be opened for a read
- **THEN** the client receives code 17, and its connection stays open

#### Scenario: A read is never resent after a frame
- **WHEN** a compute process's connection closes after one frame of a read reached the client
- **THEN** the read is refused with code 17, no other compute process receives it, and the client's connection stays
  open

#### Scenario: A hung compute process answers code 17
- **WHEN** a compute process accepts a read of `deadline_ms` 50 and sends nothing, `scan_stall_ms` is 100, and the
  read's stream holds credit
- **THEN** the frontend cancels the ComputeRead and answers the read with code 17 once 150 ms have passed, and no other
  compute process receives it

#### Scenario: A compute process waiting within the deadline is not hung
- **WHEN** a read of `deadline_ms` 40,000 waits 35 s for a close, and compute sends its first frame once the close
  arrives
- **THEN** the client receives the rows

#### Scenario: A frame restarts a hung compute's timer
- **WHEN** a compute process sends a read's frames with gaps below `scan_stall_ms`, past the read's deadline
- **THEN** the read goes on to its `LAST`

#### Scenario: A stream without credit holds off a hung compute's timer
- **WHEN** a read's stream has spent its credit, and its client takes no batch for longer than `scan_stall_ms`
- **THEN** the frontend does not answer it with code 17 on that account

### Requirement: Errors reach the client through the error table
Every refusal and failure SHALL reach the client as the wire's error code and detail from one error table, mapping each
role's errors to a code; an operating error SHALL keep the connection open, and an invariant violation in a role SHALL
answer `Unavailable` to every request it affects. When a role connection closes under a request, whether the peer went or a frame
was refused, the side that dialled SHALL answer each read and request it carried with code 17, except a read none of whose frames
reached the client, which the frontend SHALL send to the next compute process, and compute SHALL send no Scan again for such a read.
The frontend's dial of a role connection, TCP connect, Hello and SCRAM exchange together, SHALL take at most `transaction_ms`; a dial
past that bound SHALL fail the connection as a closed one does, its log naming the address and the bound.

#### Scenario: A partition's invariant violation answers Unavailable
- **WHEN** a partition stops on an invariant violation and a client writes to it
- **THEN** the client receives `Unavailable`, and requests to other partitions still succeed

#### Scenario: A role connection closing under a request answers code 17
- **WHEN** the connection between the frontend and a storage process closes under a Write and a Commit
- **THEN** the client receives code 17 for each, the Commit is in doubt, and requests to other partitions still succeed

#### Scenario: A role connection's dial past its bound closes it
- **WHEN** the frontend dials a role address whose peer accepts and says nothing, carrying a request
- **THEN** past `transaction_ms` the connection is closed, the request is told so, and the log names the address and the bound

### Requirement: Vector-index declarations wait for stage 8
In this stage the frontend SHALL refuse a declaration of a vector index with an error naming the predicate and stating
that vector indexes arrive with stage 8; a vector literal SHALL still be stored as a value.

#### Scenario: A vector index is refused
- **WHEN** a client declares a vector index on `embedding`
- **THEN** the declaration is refused with an error naming `embedding`, and the catalog is unchanged

### Requirement: Every request naming a tenant passes one gate
Every request naming a tenant, Begin, Prepare, Describe, Install, Drop, DirectWrite and ConditionalWrite, SHALL pass one gate at
the frontend before it reaches another role or any state: whether the connection's principal may do the request's action
in that tenant. A refusal SHALL be code 21 (PermissionDenied) on the request's stream, `detail` 1, `read`, for Begin,
Prepare and Describe, 3, `declare`, for an Install, 2, `write`, for a one-frame write and 4, `drop`, for a Drop. The gate SHALL fail closed on an
action or a scope it does not know. In this stage it SHALL allow every authenticated principal, and stage 7's grants
fill it in. A request the gate passes naming a tenant the metadata group does not hold SHALL then be refused with code 7
(Refused), `detail` the tenant, the message naming it, so a principal holding no grant in a number learns nothing of
whether it exists. A principal MAY hold grants in several tenants, and a connection's transactions MAY act in several,
each bound at its Begin; every request acts in one, and a request's tenant authorizes nothing, the grants do.

#### Scenario: A command of a later stage is refused on its stream
- **WHEN** an authenticated principal sends an Install naming tenant 0, a Query in an open transaction, and a Prepare of
  `language` 1
- **THEN** each is refused with code 7 on its stream, the message naming the command and its stage, the Query's transaction
  stays open, and the connection stays open

#### Scenario: The gate allows an authenticated principal in this stage
- **WHEN** an authenticated principal sends a Begin naming tenant 0
- **THEN** the gate passes it, and the Begin is answered

#### Scenario: An unknown tenant is refused after the gate
- **WHEN** an authenticated principal sends a Begin, a Prepare, a Describe, an Install, a Drop, a DirectWrite and a
  ConditionalWrite, each naming tenant 9, which the metadata group does not hold
- **THEN** each is refused with code 7, `detail` 9, the message naming tenant 9, and no other role sees it

#### Scenario: A principal the gate denies learns nothing of a number
- **WHEN** the gate, built over an authorizer denying `ann` every action, is asked for tenant 0, which exists, and
  tenant 9, which does not
- **THEN** each request is refused with code 21, `detail` its action, and the two refusals differ in nothing else

#### Scenario: The gate fails closed
- **WHEN** the gate is asked of an action or a scope it does not know
- **THEN** it denies, and the request is refused with code 21

#### Scenario: A denied request reaches no role
- **WHEN** the gate denies a DirectWrite
- **THEN** it is refused with code 21, `detail` 2, and no partition sees it

### Requirement: The frontend bounds what a connection's reads hold back
The frontend SHALL bound the Write and Operation frames it holds for a connection behind that connection's reads by
`held_bytes`, the `[endpoint]` setting, default 4,194,304 bytes. One past it SHALL be refused with code 15 (Limit),
`detail` the bound, on its transaction's stream, ending the transaction.

#### Scenario: A write past `held_bytes` is refused
- **WHEN** `held_bytes` is 1,024, a read of a transaction is open, and the client sends Writes that the read holds back
  past 1,024 bytes
- **THEN** the Write past the bound is refused with code 15, `detail` 1,024, on the transaction's stream, and the
  transaction ends

#### Scenario: Credit still drains a read under held writes
- **WHEN** a read holds Writes back, and the client grants the read credit
- **THEN** the frontend reads the Credit and relays it, the read reaches its `LAST`, and the held Writes are then sent

### Requirement: The frontend bounds what a connection has on its way to a role
The frontend SHALL count, per client connection, what it keeps for that connection on the way to a role, as `PROTOCOL.md` §11.12,
*Frames on their way*, lists it, against `held_bytes`. Once the count reaches the bound it SHALL stop reading that connection until
the count falls below it, SHALL go on sending what it answers, and SHALL refuse nothing for the bound. A transaction's limit SHALL end
what a stopped partition holds back and take back what it queued, and a one-frame write still unsent at `transaction_ms` SHALL be
taken back and answered code 4.

#### Scenario: A client past its bound stops being read until its partition reads again
- **WHEN** `held_bytes` is 4,096, partition 1 stops reading, a client pipelines Writes for partition 1 on four streams past the bound while its Commit on partition 2 waits for its reply, and sends a Begin behind them
- **THEN** the frontend counts at most the bound and one frame for it, reads no further frame of it, and sends it the reply of its Commit, another client commits on partition 2, and when partition 1 reads again the Begin is answered, each stream's Writes reach partition 1 in the order sent, and each Commit is answered

#### Scenario: A transaction's limit ends what a stopped partition holds back
- **WHEN** `held_bytes` is 4,096, a partition stops reading, and a client pipelines Writes for it past the bound and passes `transaction_ms`, a Begin behind them
- **THEN** each transaction is answered code 4, the frontend counts nothing for the client, and the Begin is answered while the partition still reads nothing

#### Scenario: What never left is taken back
- **WHEN** a partition stops reading with its socket full, a client's one-frame writes wait behind it past `transaction_ms`, and another client binds and aborts behind them
- **THEN** each one-frame write that never left is answered code 4, the frontend counts nothing for the client, and when the partition reads again it sees none of those, no Bind and no Cancel, and answers the ones that left

### Requirement: The frontend times a read-write transaction
The frontend SHALL time a read-write transaction from its Begin's arrival. Past `transaction_ms`, unless its Commit has
already been relayed, it SHALL cancel the Bind stream, if any, and answer code 4 (PastLimit), `detail` the limit, on the
transaction's stream. A partition's own timer, started at the Bind's arrival, ends a transaction whose frontend no
longer speaks for it.

#### Scenario: An unbound transaction passes its limit
- **WHEN** a read-write transaction begins and sends nothing for `transaction_ms`
- **THEN** the frontend answers code 4 on its stream, with no partition having seen it

#### Scenario: A bound transaction passes its limit
- **WHEN** a read-write transaction bound to partition 1 sends nothing for `transaction_ms` from its Begin
- **THEN** the frontend cancels the Bind stream, which aborts it at the partition, and answers code 4 on its stream

#### Scenario: A read holding a write back is named at the limit
- **WHEN** a read of a transaction has spent its credit, the client sends a Write and a second read, waits on the second read,
  and grants no credit for `transaction_ms`
- **THEN** the frontend answers code 4, the message naming the first read's stream and that it had no credit

#### Scenario: A relayed Commit is not timed
- **WHEN** a transaction's Commit has been relayed, and `transaction_ms` passes before its reply
- **THEN** the client receives the Commit's reply

### Requirement: A Commit nothing bound is answered by the frontend
The frontend SHALL answer itself a Commit of a read-write transaction no request has bound and holding no declaration,
since it reaches no partition: `commit_version` 0, the token zero and no results, storing nothing. The session's counter
moves as after any Commit reply. A resend of it SHALL be answered code 18, and running the transaction again commits
nothing as well.

#### Scenario: An empty transaction commits at the frontend
- **WHEN** a client begins a read-write transaction and sends Commit through its session with nothing else
- **THEN** the frontend answers `commit_version` 0, the token zero and no results, and no partition sees it

#### Scenario: A resent empty Commit answers not committed
- **WHEN** that Commit's reply is lost, and the client sends it again
- **THEN** every partition answers 18, and the client receives code 18

### Requirement: The endpoint asks the system group
The frontend SHALL hold no copy of what the system group decides. Each endpoint core SHALL hold a role connection of
its own to the system group at `cluster.system_address` and SHALL ask on it: Session to register a session whose
identity it drew, answering the client's Register once the Session is answered; Check before it routes a Commit, a
Drop, a DirectWrite, a ConditionalWrite or an Ingest through a session, kind 2, and before it asks a partition a
Recall, kind 1, routing the request only once its check passed and answering the client with the code a failed check
gave; and Ended when a checked request ended before reaching a partition, its partition answered it, for an Ingest its last share's, or the role connection carrying it closed. When every partition has answered a Recall with 18, the core SHALL send a second Check of kind 1 and answer 12 on outcome 12, 21 on outcome 21, 14 on outcome 14, and 18 on outcome 1. A core SHALL gather the Endeds of one turn into one frame and then the Checks of that turn into one frame, one check per session, a further request through a session waiting for the next frame, each frame holding as many checks, or sessions, as the role connection's `max_frame` takes, in ascending order of their sessions, those past that waiting for the next turn, so an eviction takes effect for the next commit on any core and a commit's check costs one round trip per turn. A turn SHALL be one run of the task that sends on the core's connection to the system group: it takes every Ended and every Check the core's requests have queued since its last run, a request queued while it sends waits for its next run, and it waits for no reply (`PROTOCOL.md` section 11.5). A core whose connection to the system group closed SHALL refuse what needs a check with code 17 until it has dialled again (system-group, "A lost system group is waited for until the deadline"), and SHALL send the Ended of a request that ends meanwhile on its next connection.

#### Scenario: An eviction reaches every core at once
- **WHEN** a session is evicted while a connection on another core holds it, and that connection commits next
- **THEN** the commit is refused with the eviction's error, and no partition sees it

#### Scenario: A blocked commit does not block other connections
- **WHEN** one connection's commit waits on its Check
- **THEN** reads and writes of other connections on that core go on

#### Scenario: One turn's commits share one Check frame
- **WHEN** eight connections of one endpoint core each queue the check of a Commit through a session of their own before
  the core's sending task runs next
- **THEN** the core sends one Check frame holding eight checks, in ascending order of their sessions, and routes each
  Commit by its own outcome

#### Scenario: A restarted frontend serves the sessions it served before
- **WHEN** a client registers a session and commits request 1, the frontend is killed and restarted, and the client
  reconnects and commits request 2 through the same session
- **THEN** request 2 commits, and the frontend replayed no log and read no file of its own

#### Scenario: A request that ends during an outage is ended on the next connection
- **WHEN** a core's connection to the system group closes while a Commit of a session is at its partition, the
  partition answers the Commit, and the core dials again
- **THEN** the client receives the Commit's reply, the core sends an Ended naming the session on the new connection,
  and the session's next Commit passes its check

#### Scenario: A resend on another core is told the original is on its way
- **WHEN** a Commit of a session is at its partition, its reply not yet sent, and the client resends it on a connection
  to another endpoint core
- **THEN** the resend is answered code 14, and no partition is asked a Recall

#### Scenario: Two requests of one session in one turn take two frames
- **WHEN** a Commit and a resend through one session arrive at one core within one turn
- **THEN** the turn's Check frame holds one check of that session, the second request's check goes in the next frame,
  and the core answers neither by itself

### Requirement: An endpoint core keeps a view of what the closes announce
Each endpoint core SHALL subscribe to the sequencer's closes on its connection to the system group, with `latest` set since it holds no close of its own, opening a subscription the sequencer ended as no longer held the same way, and SHALL keep a view: the latest closed and the open epoch, the placement in effect with the tenants its runs list, read again with
Placement when a close assigns a placement version, each tenant's catalog versions with the epoch each takes effect
at, as the closes announce them, the Catalog replies name them and its own Declare replies imply them, a version
committed as pending holding none until a close announces it or a Catalog reply names its epoch, from the highest
version in effect at the open epoch on; each tenant's current version, the highest of them; and the schema id of each version it read with Catalog, which it
computes from the version's bytes. A subscription opened again with `latest` skips closes and with them their announcements: after its first close arrives the core SHALL read Placement again, and SHALL read again with Catalog the current version of each tenant its view holds, before it answers from the view. A Begin and the gate's question whether a tenant exists SHALL be answered from the view, asking no other process, so one connection's snapshots never go back, while two connections may answer at an epoch and then the one before and only a token carries the floor from one to the other (decided 2026-11-12). A token whose epoch is above the core's open epoch SHALL not be refused: a Begin naming it SHALL be held until the core's stream delivers the close before the token's epoch, a close that has arrived when the deadline is reached SHALL be taken first and the Begin answered, the Begin SHALL be refused with code 24 only when its deadline is reached with that close not arrived, and a one-frame write naming it SHALL go to its partition. A Describe SHALL ask the system group which version to report, by a Catalog request of `version` 0 naming the latest closed epoch. Nothing SHALL be decided from the view alone, apart from the early refusals of `PROTOCOL.md` sections 6.5 and 6.8, which read a tenant's version in effect at the open epoch: the highest version taking effect at or below it, where the view holds every version above it, none of them in effect yet, and none where the view cannot name it, the tenant never read with Catalog or a version above the highest in effect never learned. A declaring Commit built on a version the
view holds is refused by the system group when that version is no longer current (schema-changes), and the frontend
SHALL pass the refusal on.

#### Scenario: A Begin asks no other process
- **WHEN** an authenticated client sends a Begin naming tenant 0
- **THEN** the frontend answers with the read version of the latest close its view holds, and sends no frame to the
  system group for it

#### Scenario: A stale view decides nothing
- **WHEN** one core commits a declaring transaction that makes version 4 of `t1`, and before the next close another
  core commits a declaring transaction built on version 3
- **THEN** the second Commit is refused with code 2, kind 5, the message naming versions 3 and 4, and the client's
  retry after the close builds on version 4

#### Scenario: A core that starts late begins at the latest close
- **WHEN** an endpoint core subscribes with `latest` after the sequencer has closed 40 epochs and trimmed the first 30
- **THEN** the first close on its stream is that of epoch 40, and its view holds epoch 40 before it answers its first
  Begin

#### Scenario: A token ahead of the core's view waits and is not refused
- **WHEN** a client's commit returned a token of epoch 9 while the core's stream has delivered the close of epoch 7, and
  the client sends a Begin naming the token on that core
- **THEN** the Begin is answered once the close of epoch 8 arrives, and is refused with code 24 only if its deadline
  is reached with that close not arrived

#### Scenario: A token carries the floor to another connection
- **WHEN** a client reads at epoch 9 on one connection, commits there, and begins a transaction naming the commit's
  token on a connection to a core whose view holds epoch 8
- **THEN** the second transaction's snapshot is at or above the token's epoch

#### Scenario: Describe never reports a version being checked
- **WHEN** a tightening of `nick` is being checked, and a client describes the tenant on a core that did not send the
  Declare
- **THEN** the Describe reports the version before it

#### Scenario: A subscription opened again reads what its skipped closes announced
- **WHEN** a core's view holds the close of epoch 7 and version 3 of `t1`, its subscription is opened again with
  `latest`, and the first close on the new stream is that of epoch 12, version 4 of `t1` and a placement version having
  been announced by the closes of epochs 8 to 11
- **THEN** the core reads Placement and, with Catalog, the current version of `t1` before it answers its next Begin,
  and its view then holds epoch 12, version 4 of `t1` and the placement in effect

### Requirement: The frontend finds the roles in Members
The frontend SHALL accept no role connection and SHALL send no Join. It SHALL read Members from the system group at
its start, again until Members lists every partition of the placement or `[cluster] join_deadline_ms` has passed,
again when one of its role connections closes, and again before it refuses a read for want of a compute process; and
each endpoint core SHALL dial the addresses Members names. A compute process that joined while others served SHALL be
found at the next of those reads.

#### Scenario: The frontend listens at no role address
- **WHEN** the frontend has started
- **THEN** it listens at its client address and its observability address alone

#### Scenario: A storage process restarted at its address is dialled again
- **WHEN** a storage process restarts at the same role address while the frontend runs
- **THEN** the frontend's connection to it closes, the frontend reads Members again and dials the address, and its next
  write to that partition is answered

#### Scenario: A compute process that joins later is found before a read is refused
- **WHEN** the only compute process the frontend knows stops, another has joined the system group meanwhile, and a
  client sends a Read
- **THEN** the frontend reads Members, dials the new process and relays the Read to it

