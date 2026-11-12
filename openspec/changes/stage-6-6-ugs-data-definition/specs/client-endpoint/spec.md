## MODIFIED Requirements

### Requirement: Vector-index declarations wait for stage 8
The frontend SHALL refuse a declaration of a vector index made through the write path, a change record of kind 8 with
its vector flag in a tenant with no schema, with an error naming the predicate and stating that vector indexes arrive
with stage 8; a vector literal SHALL still be stored as a value. An Install whose sources declare `@index(ann)` SHALL be
accepted: the declaration is recorded in the catalog version and reported with `UGS0902`, and no index is built
(ugs-semantics, "A construct of a later stage is compiled, recorded and reported").

#### Scenario: A vector index is refused
- **WHEN** a client declares a vector index on `embedding`
- **THEN** the declaration is refused with an error naming `embedding`, and the catalog is unchanged

#### Scenario: An Install declaring a vector index is accepted
- **WHEN** a client installs a schema declaring `attribute Embedding = Vector<F32, 768> @index(ann, metric: cosine)`
- **THEN** the Install succeeds, its warnings hold `UGS0902` naming `Embedding` and stage 8, and no vector index exists

### Requirement: Every request naming a tenant passes one gate
Every request naming a tenant SHALL pass one gate, Begin, Prepare, Describe, Install, Drop, DirectWrite,
ConditionalWrite, Purge and Migrate alike, at
the frontend before it reaches another role or any state: whether the connection's principal may do the request's action
in that tenant. A refusal SHALL be code 21 (PermissionDenied) on the request's stream, `detail` 1, `read`, for Begin,
Prepare and Describe, 3, `declare`, for an Install and a Migrate, 7, `purge`, for a Purge, 2, `write`, for a one-frame write and 4, `drop`, for a Drop. The gate SHALL fail closed on an
action or a scope it does not know. In this stage it SHALL allow every authenticated principal, and stage 7's grants
fill it in. A request the gate passes naming a tenant the metadata group does not hold SHALL then be refused with code 7
(Refused), `detail` the tenant, the message naming it, so a principal holding no grant in a number learns nothing of
whether it exists. A principal MAY hold grants in several tenants, and a connection's transactions MAY act in several,
each bound at its Begin; every request acts in one, and a request's tenant authorizes nothing, the grants do.

#### Scenario: A command of a later stage is refused on its stream
- **WHEN** an authenticated principal sends a Query of `language` 1 in an open transaction and a Prepare of
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

## ADDED Requirements

### Requirement: The frontend serves Install
The frontend SHALL serve an Install that passes the gate on its compile thread and through the metadata group's owner:
the compile of the tenant's sources with those sent and its verification on the compile thread, and the allocation
of prefixes and predicate sequences and the commit of the pending version by the owner, answering as ugs-catalog
states, and no other role SHALL see the request. The compile thread SHALL be started at startup and fed over a bounded
queue, an Install finding the queue full refused with code 5 (Busy), `detail` 0, the message naming the compile queue, and a
compile SHALL run off the connection's turn and off the owner's, so other streams, other connections and every ask of
the owner are served while it runs, and SHALL be bounded by the source bounds of ugs-syntax.

#### Scenario: An Install is answered by the frontend
- **WHEN** an authenticated principal installs the aviation schema into tenant 0
- **THEN** the reply names the version and its effective epoch, and no partition and no compute process saw a frame of
  the request

#### Scenario: Other streams are served during a compile
- **WHEN** a connection installs Gen5 and, on another stream, begins a transaction and reads
- **THEN** the read is answered before the Install's reply

### Requirement: The frontend compiles a UQL request and sends it as frames
The frontend SHALL compile a Query of `language` 3 against the catalog version in effect for the transaction's tenant
(uql-crud), on the connection's core, into buffers the connection holds from its start, sized by `max_frame` and the
nesting bound of 64, which each request clears and reuses. It SHALL send a mutation's change records and operations to the transaction's partition as it relays a
client's Write and Operation frames, binding the transaction by the first subject as a client's write binds it, and
SHALL route a query's pattern reads to compute as it routes a client's Read. A Query of `language` 1 or 2, and a
Prepare of `language` 1, 2 or 3, SHALL be refused with code 7 on its stream, the message naming the stage that serves
it.

#### Scenario: A UQL insert reaches the partition as a Write
- **WHEN** a client sends `insert Company { id: "ORG:acme" }` in a read-write transaction
- **THEN** the partition owning the bucket of `…#Node/ORG%3Aacme` receives one Write holding the company's triples, and
  the transaction is bound to it

#### Scenario: A UQL request reaching a second partition is refused
- **WHEN** a read-write transaction bound to partition 1 sends an insert whose subject's bucket partition 2 owns
- **THEN** it is refused with code 23 naming partitions 1 and 2, and the transaction aborts

#### Scenario: A GQL Query still waits for stage 7
- **WHEN** a client sends a Query of `language` 1 in an open transaction
- **THEN** it is refused with code 7 on its stream, the message naming Query and stage 7, and the transaction stays open
