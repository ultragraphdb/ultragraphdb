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
address SHALL be throttled.

#### Scenario: A wrong password is refused without detail
- **WHEN** a client authenticates as `ann` with a wrong password, and as an unknown name
- **THEN** both are refused with the same authentication error

#### Scenario: A data request before authentication is refused
- **WHEN** a client sends a Read before authenticating
- **THEN** the request is refused with the authentication error, and the connection stays open for authentication

### Requirement: Requests are routed to the partitions and compute that serve them
The frontend SHALL convert a request's terms, find each subject's bucket and partition from its placement map, and send
a write to its partition's storage process and a read to compute; a reply SHALL reach the client on the request's
stream. A partition's refusal naming another owner SHALL make the frontend refresh its placement map and resend once.

#### Scenario: A write reaches its partition
- **WHEN** a client writes `<http://ex.org/a> <http://ex.org/knows> <http://ex.org/b>`
- **THEN** the frontend sends it to the partition owning the bucket of `http://ex.org/a`, and the commit's reply reaches
  the client

#### Scenario: A stale placement map is corrected
- **WHEN** a partition refuses a write naming its new owner
- **THEN** the frontend refreshes its map, resends the write to the owner once, and the client receives the owner's reply

### Requirement: Reads are spread over the compute processes
The frontend SHALL send each read to the next compute process of its members, round-robin per endpoint core, skipping one
that does not answer for that read, and SHALL refuse the read with `Unavailable` when none is left.

#### Scenario: Reads alternate between two compute processes
- **WHEN** two compute processes have joined and a connection sends two reads
- **THEN** each process receives one

#### Scenario: A silent compute process is skipped
- **WHEN** one of two compute processes stops answering and a read is sent to it
- **THEN** the frontend sends the read to the other, and the client receives its rows

#### Scenario: No compute process answers
- **WHEN** no compute process answers a read
- **THEN** the client receives `Unavailable`, and its connection stays open

### Requirement: Errors reach the client through the error table
Every refusal and failure SHALL reach the client as the wire's error code and detail from one error table, mapping each
role's errors to a code; an operating error SHALL keep the connection open, and an invariant violation in a role SHALL
answer `Unavailable` to every request it affects.

#### Scenario: A partition's invariant violation answers Unavailable
- **WHEN** a partition stops on an invariant violation and a client writes to it
- **THEN** the client receives `Unavailable`, and requests to other partitions still succeed

### Requirement: Vector-index declarations wait for stage 8
In this stage the frontend SHALL refuse a declaration of a vector index with an error naming the predicate and stating
that vector indexes arrive with stage 8; a vector literal SHALL still be stored as a value.

#### Scenario: A vector index is refused
- **WHEN** a client declares a vector index on `embedding`
- **THEN** the declaration is refused with an error naming `embedding`, and the catalog is unchanged
