## ADDED Requirements

### Requirement: The frontend serves the binary protocol's clients
The frontend SHALL accept client connections at its configured address, run the handshake and version negotiation of
the wire protocol, and serve each connection on one core's runtime. It SHALL bound its connections and each
connection's streams, prepared statements and pending requests, answering past a bound with the wire's limit error.

#### Scenario: Connections past the bound are refused
- **WHEN** the frontend allows 2 connections and a third client connects
- **THEN** the third connection is refused with the limit error, and the first two keep working

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
