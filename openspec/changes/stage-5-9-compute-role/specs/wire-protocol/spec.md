## ADDED Requirements

### Requirement: The node list's frames are bodies of the protocol
The protocol SHALL carry Nodes (27), from a compute node to an authenticated client on stream 0 with `flags` 0, and
Roster (537), between a compute node and the system group, with the bodies `PROTOCOL.md` sections 5.4 and 13.26
state: a `version`, `self`, the nodes of 16 bytes each, their tag references, and the two string lists of addresses
and tags. A Join SHALL carry `state` and, for compute, the advertised address and the tags in place of a role address;
an EpochClose SHALL carry `nodes`; a Members reply SHALL hold storage members alone. ComputeRead (523) and role kind 1
SHALL be retired: the number is refused as an unknown command and never used again, and the SCRAM usernames of role
connections are `compute` and `storage`. Every decoder SHALL enforce its body's order of checks, and a builder SHALL
refuse to build what its decoder refuses.

#### Scenario: A Nodes frame's addresses match its nodes
- **WHEN** a Nodes frame holds three nodes and two addresses, or an address that is not `tcp://host:port`
- **THEN** the first is refused with `CountMismatch` naming `addresses`, the second with `FieldValue` naming `address`
  and the node's index

#### Scenario: A list within its bounds fits one frame
- **WHEN** a list of 1,024 nodes, each with an address of 265 bytes and eight tags out of 256 of 32 bytes, is built
- **THEN** the Nodes frame is at most 524,288 bytes

#### Scenario: A Roster reply may hold no node
- **WHEN** the last compute node has left the list at version 30, and a Roster is read
- **THEN** the reply holds `version` 30, no node and `self` `ABSENT`, and decodes

#### Scenario: A compute Join names no role address
- **WHEN** a Join of kind 2 holds a role address in section 0, or no advertised address
- **THEN** the first is refused with `CountMismatch` naming `address`, the second with `FieldValue` naming `advertised`

#### Scenario: Members holds no compute node
- **WHEN** a Members reply holds a member of kind 2
- **THEN** it is refused with `FieldValue` naming `kind`

#### Scenario: Each changed body has a golden frame
- **WHEN** the golden frames of Nodes, Roster, a compute node's Join, a storage process's Join and an EpochClose naming
  a change are decoded and built again
- **THEN** each decodes to the fields its generator names and builds to the same bytes

## MODIFIED Requirements

### Requirement: Data requests, prepared statements and describe
A read SHALL be a quad pattern with each position bound or unbound, a limit and a resume key, naming its transaction,
whose Begin names the level, session token and deadline the read takes; a row SHALL name its statement by its quad and,
when the statement holds a statement ID, its reifier IRI, never by the statement ID. A write SHALL be a changeset, in
its transaction's tenant, addressing a statement's properties, or one of several parallel edges, through the statement's
reifier IRI, and a statement without one by its triple term and graph; a commit-time operation SHALL be its own request,
and its result in the Commit reply SHALL name the node it found or created by its IRI or skolem IRI. A tenant SHALL bind
per Begin, so one connection's transactions MAY act in several. A statement SHALL be prepared once per connection, which
holds it, in the tenant its Prepare names, from a quad pattern with parameter slots, answered with a statement id, its
parameter and result column types and a schema id, and executed with parameters bound by position; Execute SHALL refuse
a statement prepared in another tenant than its transaction's with code 10 (UnknownStatement), `detail` the statement;
after a new catalog version of its tenant its schema id SHALL no longer match, and an execution SHALL be refused as
stale, while recording a default entry (metadata-group, "Catalog versions take effect at an epoch boundary") SHALL keep
the schema id; a literal bound at a slot that takes a node SHALL be refused. A resume key SHALL continue its own request
only, and rows SHALL come in the store's key order. Compute SHALL answer Describe itself, from the catalog version in
effect, read from the system group with Catalog, and the tenant's default-entry records, read with Defaults, asking no
partition, as far as the connection's principal may read it, so it names what is declared and recorded rather than what
data holds: `read_version` the version bound of the latest epoch the sequencer closed, or of epoch 0 before the first
close; the entity types declared in effect at that epoch; the predicates of kind edge or either that it declares or
holds a default entry for as relationship types, those of kind property or either as property keys; and the catalog's
declarations. An answer not fitting one frame within the client's `max_frame` SHALL be refused with code 6 (TooLarge),
`detail` that `max_frame`.

#### Scenario: A stale statement is refused
- **WHEN** a statement is prepared, the catalog of its tenant changes, and the statement is executed
- **THEN** the execution is refused as stale, and preparing it again succeeds

#### Scenario: Describe answers from the metadata group
- **WHEN** `name` is declared a property with a `term` index, `score` holds a default entry, and a client sends Describe
  for the tenant
- **THEN** compute answers asking no partition: `read_version` is the version bound of the latest closed epoch,
  the declarations list `name` with its `term` index, the property keys hold `name` and `score`, and the relationship
  types hold `score`

#### Scenario: A literal at a node slot is refused
- **WHEN** a statement's subject parameter is bound to a literal
- **THEN** the execution is refused with code 11, `detail` the statement

#### Scenario: A commit-time operation's node is named by its IRI
- **WHEN** a transaction's insert-if-absent finds the node `http://ex.org/x` holding the value, and the transaction
  commits
- **THEN** the Commit reply names `http://ex.org/x` by its text, and holds no system ID

#### Scenario: A Describe past the client's frame is refused
- **WHEN** a tenant's Describe answer does not fit one frame within the client's `max_frame`
- **THEN** it is refused with code 6, `detail` that `max_frame`

#### Scenario: A statement prepared in another tenant is refused
- **WHEN** a connection prepares a statement in tenant 1 and executes it in a transaction of tenant 0
- **THEN** the execution is refused with code 10, `detail` the statement, and the transaction goes on

#### Scenario: One connection's transactions act in two tenants
- **WHEN** a connection begins one transaction in tenant 0 and another in tenant 1, and each writes and reads `<a> <p>
  <b>`
- **THEN** each reads its own statement, and neither reads the other's

### Requirement: Requests name terms by their text
A request body that carries RDF terms SHALL carry them in a term table of its own, one entry per distinct term, each a
kind and offsets into the body's text section: an IRI by its text, a blank node by its label scoped to the request, a
literal by its lexical form with its datatype's IRI (none for `xsd:string`) or its language tag and base direction, and
a triple term by the indices of three earlier entries of the same table. Every term the body names SHALL be an index
into that table. A receiver SHALL refuse an entry that repeats an earlier one, a triple term naming an entry at or after
its own, and a triple term nested deeper than 32. A client names a term only by its text; it names a statement without a
statement ID by its quad, the triple term of its subject, predicate and object with its graph, and a statement with one
by its reifier's skolem IRI, as RDF 1.2 reification does; result batches name IRIs by their text through the batch's
dictionary: no system ID crosses to a client. Compute SHALL refuse a term breaking a slot's rule with Error 7 (Refused)
on the request's stream, `detail` the term's index and the message naming the rule, a literal parameter's code 11 coming
first where it applies: a pattern's graph or subject slot takes an IRI; a pattern's object slot takes no blank node's
label, which a request names again by its skolem IRI, and no triple term; the graph or subject slot of a change or an
operation takes an IRI or a blank node; and no slot of a change or an operation takes a triple term, since no statement
holds one. In a change or an operation, an IRI under a tenant's skolem base,
`https://w3id.org/ultragraphdb/ns/tenant/{n}/`, SHALL be refused the same way unless the dictionary of the partition
owning the bucket its text states holds it; in a pattern, an unheld skolem IRI SHALL match nothing, as any IRI nobody
holds. Compute SHALL compare the base exactly. One under it naming another tenant than the request's, whose tail is not
`{bucket}-{random}`, or whose bucket lies outside the tenant's run, is an IRI nobody holds: compute SHALL refuse it in a
change or an operation, and SHALL answer a read binding it itself with no row, binding nothing.

#### Scenario: A skolem IRI nobody can hold binds nothing
- **WHEN** a read-write transaction's first request is a Read binding its subject to an IRI under tenant 7's skolem base in
  tenant 0, and its next request is a Write of `<a>`
- **THEN** the Read is answered one batch flagged `LAST`, `rows` 0, no role sees it, and the transaction binds to `<a>`'s
  partition

#### Scenario: A skolem IRI nobody can hold is refused in a change
- **WHEN** a Write names as a subject an IRI under the tenant's skolem base whose tail states no bucket, and another under
  tenant 7's
- **THEN** each is refused with Error 7, `detail` the term's index, and no partition is asked

#### Scenario: The same path under another base is an ordinary IRI
- **WHEN** a Write names `https://example.org/tenant/0/.well-known/genid/00abcd-0123456789abcdef0123456789`
- **THEN** it is routed by its text's hash and stored as any IRI

#### Scenario: A request names an IRI by its text
- **WHEN** a Read binds its subject to the IRI `http://ex.org/a`
- **THEN** the view reads an IRI entry whose text is `http://ex.org/a`, borrowed from the frame

#### Scenario: A request names a blank node by its label
- **WHEN** a Write inserts a statement whose subject is the blank node labelled `b1`
- **THEN** the view reads a blank-node entry labelled `b1`

#### Scenario: A request names a typed literal by its datatype's IRI
- **WHEN** an Execute binds the literal `35` of datatype `http://www.w3.org/2001/XMLSchema#integer`
- **THEN** the view reads a literal entry of lexical form `35` and that datatype's IRI

#### Scenario: A request names a language-tagged literal
- **WHEN** an Operation's value is `chat` tagged `fr`
- **THEN** the view reads a literal entry of lexical form `chat` and language tag `fr`

#### Scenario: A request names a triple term by its parts
- **WHEN** a Write's object is the triple term of `http://ex.org/a`, `http://ex.org/knows` and `http://ex.org/b`
- **THEN** the view reads a triple-term entry naming three earlier entries, those IRIs

#### Scenario: A forward index is refused
- **WHEN** a triple term's entry names an entry at or after its own index
- **THEN** the frame is refused with an error naming the entry and the index

#### Scenario: A triple term nested deeper than the maximum is refused
- **WHEN** a request's term table holds a triple term nested 33 deep
- **THEN** the frame is refused with an error naming the entry and its depth

#### Scenario: A result names IRIs by their text
- **WHEN** a read returns rows whose subjects are `http://ex.org/a` and `http://ex.org/b`
- **THEN** the batch's dictionary holds both IRIs' texts and the rows name them by dictionary index, with no system ID

#### Scenario: A result names a blank node by its opaque skolem IRI
- **WHEN** a read returns a row whose subject is a blank node
- **THEN** the batch names it, tagged as a blank node, by its skolem IRI `…/.well-known/genid/{bucket}-{random}`, whose
  text holds no type prefix and no sequence of its system ID

#### Scenario: A literal at a pattern's subject is refused
- **WHEN** a Read's subject slot holds a literal, and another's graph slot a blank node's label
- **THEN** each is refused with Error 7, `detail` the term's index, the message naming the rule

#### Scenario: A label or a triple term at a pattern's object is refused
- **WHEN** a Read's object slot holds a blank node's label, and another's a triple term
- **THEN** each is refused with Error 7, `detail` the term's index, the message naming the rule

#### Scenario: A triple term in a change is refused
- **WHEN** a Write's object is a triple term
- **THEN** the Write is refused with Error 7, `detail` the term's index, and the transaction ends

#### Scenario: A forged skolem IRI in a pattern matches nothing
- **WHEN** a Read names as its subject a skolem IRI no dictionary holds
- **THEN** the read answers one batch flagged `LAST` with no row, and its transaction goes on

### Requirement: A write commits in one frame
DirectWrite SHALL carry a changeset, and ConditionalWrite commit-time operations with any changes, each under a session,
a request number, a tenant and a session token, each answered by a Commit. Each SHALL open a stream of its own and need
no Begin. Its command header SHALL hold `principal` at offset 56: 0 from a client, refused at the client endpoint with
`FieldValue` naming `principal`. Compute SHALL relay the frame on a new stream with `principal` filled with the
connection's principal and every other command-header byte and the body unchanged, a resend the same way, and the
partition's write-set record SHALL name that principal.

#### Scenario: A DirectWrite is relayed with its principal
- **WHEN** a client authenticated as principal 9 sends a DirectWrite through compute
- **THEN** the frame the partition receives holds the client's body byte for byte and its command header but for
  `principal` 9, its write-set record names principal 9, and the client receives a Commit

#### Scenario: A malformed write is refused
- **WHEN** a DirectWrite holds no change, or a ConditionalWrite no operation
- **THEN** it is refused with `CountMismatch`, naming `changes` or `operations`

#### Scenario: A write in one frame holds what it uses
- **WHEN** a DirectWrite or ConditionalWrite holds a name, or names a request number that does not fit its session
- **THEN** it is refused with `CountMismatch` naming `names`, or `FieldValue` naming `request`

#### Scenario: A client naming a principal is refused
- **WHEN** a client's DirectWrite holds a `principal` above 0
- **THEN** it is refused at the client endpoint with `FieldValue` naming `principal`, and no partition sees it

### Requirement: Roles exchange role commands
Roles SHALL exchange role commands in the same frames as clients, in a command range clients may not send: routing a
write or a read to a partition, a reverse read's request to the partitions an entity's referrer set names included,
joining the cluster at the system group's address and learning its members, registering a session, asking a session's
owner check and ending the mark of its commit in flight, asking for the catalog version a declaring Commit or a Drop
calls for, reading a tenant's default-entry records, registering a reference at an entity's home and its reply,
delivering epoch closes with the catalog versions they assign and the session evictions they carry, a partition's
request for a close, leasing a sequence block, reading the placement map and a catalog version, and asking for a
principal's challenge and verifying its proof. A role command arriving on a client connection SHALL be refused with a
protocol error naming the command, and a role connection SHALL authenticate with the cluster's role secret
(role-binaries, "Roles authenticate each other with the cluster's role secret") before its first role command. A role
endpoint's Welcome SHALL announce `max_frame` 1,048,576 and `max_streams` 4,294,967,295, and a role's Hello `max_frame`
1,048,576, so a relayed frame takes any size a client's may and a role's stream refusal never reaches a client as a
limit it did not pass; every Welcome's `initial_credit` SHALL be at least 1. A partition SHALL answer each Write and
Operation on a Bind stream with Held (531), once it holds the request, or with an Error; the protocol's Answered-by
column SHALL list Held for both on a Bind stream, and it SHALL refuse a Write, Operation, Commit or Abort on a stream no
Bind of kind 1 opened, and a Drop on one no Bind of kind 2 opened, with Error 1 naming the command. Entries SHALL name
the Scan's chosen `version` in every frame, a version or version bound, refused otherwise with `FieldValue` naming
`version`, as a Scan's and a Begin's `read_version` is naming `read_version`, and `through`, the batch's last range; a
Scan's range SHALL carry `after_len` and its `after`;  a Reference's `outcome` SHALL be 0, 7, 17 or 26. A dialling side
SHALL refuse an Entries frame whose `version` is not its Scan's first frame's, with `FieldValue` naming `version`, and
an entry that does not belong where its frame puts it, with `FieldValue` naming `entry order`, the entry's index: in a
run, one whose key is not the first key of a range from `range` through `through`, at or after the range of the entry
before; in a batch of one range, a key below the range's first key, or at or above its past key when it has one.

#### Scenario: A client may not send a role command
- **WHEN** a client connection sends a block-lease command
- **THEN** the frame is refused with a protocol error naming the command, and nothing is leased

#### Scenario: A Scan naming a transaction names its IRIs by their text
- **WHEN** a Scan names a transaction and the IRIs its first ranges look up
- **THEN** the view reads each IRI's text from the Scan's term table, borrowed from the frame

#### Scenario: A Scan naming no transaction holds no term table
- **WHEN** a Scan with `transaction` zero holds a term
- **THEN** it is refused with `CountMismatch` naming `terms`

#### Scenario: A Scan's term table holds at most as many terms as ranges
- **WHEN** a Scan's term table holds more terms than the Scan has ranges
- **THEN** it is refused with `CountMismatch` naming `terms`

#### Scenario: A Scan naming a transaction reads as the transaction does
- **WHEN** a Scan names a transaction with `at` not 1, a nonzero `epoch`, or a term that is not an IRI
- **THEN** it is refused with `FieldValue` naming `at`, `epoch` or `scan term`

#### Scenario: A Scan names its deadline
- **WHEN** a Scan whose `epoch` is 0 holds `deadline_ms` above 0, with a transaction or with none
- **THEN** each is refused with `FieldValue` naming `deadline_ms`

#### Scenario: A list larger than a frame continues across frames
- **WHEN** a list's object records span two Entries frames, the first with `continues` 1
- **THEN** the second's first entry holds the same key, and the records of both are the list's, in order

#### Scenario: A frame whose value continues has end 0 and an entry
- **WHEN** a frame holds `continues` 1 with `end` not 0, or with no entry
- **THEN** it is refused with `FieldValue` naming `continues`

#### Scenario: A continued value cannot stop
- **WHEN** the frame after one with `continues` 1 holds no entry, or a first key that is not the same
- **THEN** the receiver refuses it with `FieldValue` naming `entry order`

#### Scenario: An answer ending early cannot read as whole
- **WHEN** a frame carries `LAST` before the last range has ended, or the last range ends with no `LAST`
- **THEN** the receiver refuses it with `FieldValue` naming `end`

#### Scenario: An object record past its value is refused
- **WHEN** an object record's encoding or statement IDs run past the value
- **THEN** the reader refuses it with `EntryOutOfBounds`

#### Scenario: A registration is a role command
- **WHEN** a partition of one storage process registers a reference to an entity homed in another storage process
- **THEN** it sends a registration role command to that process, and receives the entity's ID, the member's generation
  and the home's latest delivered close in the reply

#### Scenario: Epoch closes reach storage in order
- **WHEN** the system group's sequencer closes epochs 1 to 5 and storage is connected
- **THEN** storage receives five epoch-close commands, in order

#### Scenario: A Write on a Bind stream is answered by Held
- **WHEN** compute sends a Write on a Bind stream, its references registered and its changes held by the partition
- **THEN** the partition answers Held on the stream, which stays open; the dialling side's check accepts it, and compute relays nothing to the client

#### Scenario: Held elsewhere is refused
- **WHEN** a Held frame arrives on a stream whose request's answers do not list it, or carries a nonzero command-header
  byte
- **THEN** the receiver refuses it as `CommandNotServed`, or with `ReservedNotZero`

#### Scenario: A Write on a stream no Bind opened is refused
- **WHEN** a storage process receives a Write on a stream no Bind opened, a Commit on one of kind 2, and a Drop on one
  of kind 1
- **THEN** each is refused with Error 1, the message naming the command

#### Scenario: A role endpoint announces its limits
- **WHEN** a role connects to a role endpoint
- **THEN** the Welcome names `max_frame` 1,048,576 and `max_streams` 4,294,967,295, and the role's Hello named
  `max_frame` 1,048,576

#### Scenario: A Welcome of no credit is refused
- **WHEN** a Welcome names `initial_credit` 0
- **THEN** it is refused with `FieldValue` naming `initial_credit`

#### Scenario: The retired relay command is refused
- **WHEN** a role connection sends command 523, which carried a read to a compute node until stage 5.9
- **THEN** it is refused as an unknown command

#### Scenario: A Scan's `after` is read from the frame
- **WHEN** a Scan's range holds `after_len` 16 and the object's encoding after its past key
- **THEN** the view reads the first key, the past key and the `after` borrowed from the frame, and the next range's
  first key starts where the `after` ends

#### Scenario: An Entries frame's run reads as its ranges
- **WHEN** an Entries frame holds `range` 2, `through` 5, `end` 1 and two entries, and a frame with `through` above
  `range` and `end` 0
- **THEN** the first is read as ranges 2 to 5, each ended, and the second is refused with `FieldValue` naming `through`

#### Scenario: A version field holds a version or a version bound
- **WHEN** an Entries frame's `version`, a Scan's `read_version` and a Begin's `read_version` each hold an odd sequence, and
  then each holds an epoch's version bound
- **THEN** the first three are refused with `FieldValue` naming the field, and the bound is read

#### Scenario: A frame of another version is refused
- **WHEN** a Scan's second Entries frame names another `version` than its first
- **THEN** the receiver refuses it with `FieldValue` naming `version`, and closes the connection

#### Scenario: An entry outside its run is refused
- **WHEN** a run's entry holds a key that is the first key of none of its ranges, or a range's entry lies below the
  range's first key
- **THEN** the receiver refuses it with `FieldValue` naming `entry order`, the entry's index, and closes the connection

#### Scenario: A Reference answers a home's failed store
- **WHEN** a home's store fails under a registration
- **THEN** the reply's `outcome` is 17, its `generation` and `close` 0, and a receiver accepts it

### Requirement: A parked request holds no role connection
A partition SHALL copy every request out of its connection's receive buffer as it arrives, hand the copy to its core,
and go on reading that connection without waiting for the core to take the request, as `PROTOCOL.md` section 11.12's
*Parked requests* states; it SHALL send each request's answer on its stream once it has one, whether the request runs at
the core's next turn or waits on a home. A stream's requests SHALL take effect and be answered in the order they were
sent, and the answers on one connection SHALL leave in the order the core gives them. A connection's held bytes, its
requests copied and not yet taken by the core together with those parked, SHALL be bounded by `storage.held_bytes`, at
which the partition stops reading that connection until they fall below it, refusing nothing.

#### Scenario: A write parked on a home's Reference holds no role connection
- **WHEN** a Write on a Bind stream waits on a home's answer, and a second request arrives on its connection
- **THEN** the second request is answered while the Write waits, and the Write is answered Held on its stream once the home answers

#### Scenario: A one-frame write parked on a home's Reference holds no role connection
- **WHEN** a DirectWrite waits on a home's answer, and a second request arrives on its connection
- **THEN** the second request is answered while the DirectWrite waits, and its Commit reply follows once the home answers

#### Scenario: A write behind a read's wait holds no role connection
- **WHEN** a Scan of a transaction waits on a home's answer, a Write follows on the transaction's stream, and a second request arrives on the connection
- **THEN** the second request is answered while the Write waits its turn, and the Write is answered Held once the home answers

#### Scenario: What follows a parked write on its stream keeps its order
- **WHEN** a Write waits on a home's answer, and a second Write and the Commit follow on its stream
- **THEN** nothing is answered on the stream until the home answers, and then Held, Held and the Commit, in that order, and the
  commit holds both writes

#### Scenario: A home's refusal drops what is parked behind the write
- **WHEN** a Write waits on a home's answer, a second Write follows on its stream, and the home refuses the registration
- **THEN** the stream ends with the home's refusal alone, and the transaction holds nothing

#### Scenario: A Cancel reaches a parked write
- **WHEN** a Write waits on a home's answer, and its stream is cancelled
- **THEN** the partition holds no such transaction by the time a later request on the connection is answered, and no Held follows
  once the home answers

#### Scenario: A connection closing with a write parked aborts its transaction
- **WHEN** a Write waits on a home's answer, and its connection closes
- **THEN** the partition no longer holds the transaction

#### Scenario: A connection at its parked bound stops reading
- **WHEN** a Write's frame takes its connection's parked bytes to `storage.held_bytes`, the Write waiting on a home's answer, and a
  second request follows on the connection
- **THEN** the second request is unanswered until the home answers and is then answered, and nothing is refused

#### Scenario: Writes parked on each other's homes both answer
- **WHEN** both storage processes hold `storage.held_bytes` of 1, and a Write on partition 1 names an entity partition 3 owns while
  a Write on partition 3 names an entity partition 1 owns
- **THEN** both are answered Held and both transactions commit

#### Scenario: A stop with a write parked answers or closes
- **WHEN** a storage process stops while a Write on a Bind stream waits on a home's answer
- **THEN** the stream is answered with code 17 or the connection closes

#### Scenario: A connection is read on while its core syncs
- **WHEN** a partition's log sync is in progress and its compute node sends ten DirectWrites on ten streams of one role connection
- **THEN** the partition reads all ten frames before the sync completes, and each is answered on its stream once its round is
  durable

#### Scenario: Requests waiting for the core count against the bound
- **WHEN** `storage.held_bytes` is 4,096, the core is in a sync, and compute sends DirectWrites of 1,500 bytes on one role
  connection
- **THEN** the partition reads three of them and stops reading that connection until the core has taken them, refusing nothing

#### Scenario: An Abort's stream ends with no frame
- **WHEN** a transaction's Abort arrives on its Bind stream while the core is in a sync
- **THEN** the stream ends with no frame once the core takes the Abort, and a later request on the connection is answered

### Requirement: A frame past its stream's credit is refused
Every receiver of Result or Entries frames, always the side that dialled, SHALL refuse a frame past its stream's credit
as a protocol violation, code 1 (Protocol) by the rule `PastCredit` naming the stream, and SHALL close the connection,
sending no Error, which only the side that accepted sends.

#### Scenario: A Result frame past its credit closes the client's connection
- **WHEN** a server sends a read's stream a Result frame with the stream's credit spent
- **THEN** the client refuses it as `PastCredit`, naming the stream, and closes the connection

#### Scenario: An Entries frame past its credit closes compute's role connection
- **WHEN** a partition sends a Scan's stream an Entries frame with the stream's credit spent, a second read's Scan being open on the connection
- **THEN** compute closes the role connection and answers both reads code 17

#### Scenario: An Entries frame past its credit closes compute's role connection
- **WHEN** a partition sends a Scan's stream an Entries frame with the stream's credit spent
- **THEN** compute refuses it as `PastCredit`, naming the stream, and closes the role connection

### Requirement: A commit in flight is in doubt once sent
When a connection is lost or closed with a commit in flight, the client SHALL answer `ConnectionLost` if the commit's
frame was never sent on any connection, and `InDoubt` if it was sent, on any attempt; an exhausted retry SHALL answer
`InDoubt`, and a Commit, Drop or one-frame write answered code 17 SHALL be in doubt the same way. The session SHALL
resolve an in-doubt commit by resending it with the same request number, which the partitions' session tables answer
(transactions, "Exactly-once commits through sessions"): the stored reply says it committed and code 18 that it did not;
code 14 says it is undecided and code 17 that a partition could not be asked, so it stays in doubt and the client
resends after a delay; codes 12 and 13 leave it unknown for good, and after 13 the counter moves above `detail`. A
resent one-frame write or Drop has no code 18: a Commit or Drop reply says it committed, codes 12, 13, 14 and 17 read as
above, codes 21 and 23, which compute gives before any partition's table decides, leave it in doubt, and any other
refusal says it did not. A commit through no session, a declaration's among them, SHALL have no resend: a lost reply or
code 17 reports its outcome as unknown.

#### Scenario: An unsent commit is lost, a sent one in doubt
- **WHEN** the connection closes once with a commit queued and unsent, and once with a commit sent and unanswered
- **THEN** the first answers `ConnectionLost`, the second `InDoubt`, and resolving the second reports whether it committed

#### Scenario: Code 17 on a commit is in doubt
- **WHEN** a client's Commit through a session is answered code 17
- **THEN** the client answers `InDoubt`, keeps the request number, and sends no other commit through the session until a
  resend resolves it

#### Scenario: A resent one-frame write refused for another reason did not commit
- **WHEN** a DirectWrite in doubt is resent and refused with code 7
- **THEN** the client reports that it did not commit

#### Scenario: A commit through no session has an unknown outcome
- **WHEN** a transaction committing through no session loses its reply
- **THEN** the client reports the outcome as unknown, and resends nothing

### Requirement: A request on a stream that cannot take it is refused
Compute SHALL refuse a Write, Operation or Abort on a new stream, and a Commit through no session on a new stream, with
Error 1 (Protocol) on that stream, the message naming the command; and a Write, Operation or Commit on a read-only
transaction's stream the same way, ending the transaction. A frame on a stream number at or below the highest the
connection opened, on a stream no longer open, is late, and a server SHALL drop it, a Commit and a Drop included. A
Write, Operation or Commit on a transaction's stream after its Commit is late, and compute SHALL drop it, the Commit
deciding the transaction. A read naming a transaction whose Error has not yet left SHALL be answered with that Error, as
the transaction's reads still open are. A dialling side SHALL take a frame on a stream it has ended, numbered below the
next stream it will open, through the frame's checks and then drop it, whatever its command, and SHALL refuse a stream
at or above that number as answering no request.

#### Scenario: A Write on a new stream is refused
- **WHEN** a client sends a Write, an Operation, an Abort, and a Commit through no session, each on a new stream
- **THEN** each is refused with Error 1 on that stream, the message naming the command

#### Scenario: A Commit in a read-only transaction is refused
- **WHEN** a client sends a Write, an Operation and a Commit, each on the stream of its own read-only transaction
- **THEN** each is refused with Error 1, the message naming the command, and the transaction ends

#### Scenario: A Commit crossing its stream's Error is late
- **WHEN** a client's Commit crosses the Error that ended its transaction's stream
- **THEN** the server drops the Commit, and the connection stays open

#### Scenario: A request behind its transaction's Commit is late
- **WHEN** a client sends a Write and a second Commit on its transaction's stream after the Commit compute relayed,
  and the partition then answers that Commit
- **THEN** the server drops both, and the client receives the Commit's reply

#### Scenario: A read crossing its transaction's Error takes that Error
- **WHEN** a client's Read crosses the Error that ended its transaction's stream
- **THEN** the read is answered with that Error, as the transaction's reads still open are

#### Scenario: A late frame reaches the client
- **WHEN** a Result frame arrives for a read the client has cancelled
- **THEN** the client checks the frame and drops it, and the connection stays open

#### Scenario: A frame on a stream not yet opened is refused
- **WHEN** a frame arrives on a stream at or above the next stream the client will open
- **THEN** the client refuses it, and closes the connection

### Requirement: The client's timeout leaves a read its deadline
The Rust client's timeout for a read SHALL run from the later of its last progress and the Begin's deadline, SHALL be at
least 30 s, the most `scan_stall_ms` holds, and SHALL not run for a stream whose credit is spent, since a read's silence
within its transaction's deadline is a wait for a close and a hung compute node is answered with code 17 by the deadline
plus `scan_stall_ms`.

#### Scenario: A long deadline survives the client's timeout
- **WHEN** a read-only transaction's deadline is 40 s, and its read waits 35 s for a close, the client's timeout set at
  30 s
- **THEN** the connection stays open, and the read answers once the close arrives

#### Scenario: A stream without credit runs no timeout
- **WHEN** a read's stream has spent its credit, and its caller takes no batch for longer than the timeout
- **THEN** the connection stays open
