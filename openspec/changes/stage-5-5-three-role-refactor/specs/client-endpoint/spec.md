## ADDED Requirements

### Requirement: The frontend serves the binary protocol's clients
The frontend SHALL accept client connections at its configured address, run the handshake and version negotiation of
the wire protocol, and serve each connection on one core's runtime. It SHALL bound its connections and each
connection's streams, prepared statements and pending requests, answering past a bound with the wire's limit error.

#### Scenario: Connections past the bound are refused
- **WHEN** the frontend allows 2 connections and a third client connects
- **THEN** the third connection is refused with the limit error, and the first two keep working

### Requirement: One owner answers for the metadata group
The frontend SHALL run its metadata group, sequencer and session registry on one owning core, and its connections on
endpoint cores, which SHALL hold no copy of what the owner decides. An endpoint core SHALL ask the owner by message for a
session's owner check on every commit and for a principal's verifier on every authentication, so an eviction takes effect
for the next commit on any core. An endpoint core MAY gather the checks of one turn into one message.

#### Scenario: An eviction reaches every core at once
- **WHEN** a session is evicted while a connection on another core holds it, and that connection commits next
- **THEN** the commit is refused with the eviction's error, and no partition sees it

#### Scenario: A blocked commit does not block other connections
- **WHEN** one connection's commit waits on its owner check
- **THEN** reads and writes of other connections on that core go on

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
A connection SHALL authenticate with SCRAM-SHA-256 against a principal the metadata group holds before any data
request, and every request SHALL run with that principal's rights. A failed authentication SHALL be answered with the
wire's authentication error without saying whether the name or the proof was wrong, and repeated failures from one
address SHALL be throttled. The server SHALL answer an unknown name with a verifier derived from the secret the metadata
group holds (metadata-group, "Principals live in the metadata group"), so a name's salt tells no prober whether it exists.

#### Scenario: A wrong password is refused without detail
- **WHEN** a client authenticates as `ann` with a wrong password, and as an unknown name
- **THEN** both are refused with the same authentication error

#### Scenario: A data request before authentication is refused
- **WHEN** a client sends a Read before authenticating
- **THEN** the request is refused with the authentication error, and the connection stays open for authentication

#### Scenario: An unknown name answers as a known one does
- **WHEN** a client asks to authenticate as an unknown name, twice, across a restart of the frontend
- **THEN** both challenges carry the same salt and iteration count, and the exchange fails with the same authentication
  error as a wrong password

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
