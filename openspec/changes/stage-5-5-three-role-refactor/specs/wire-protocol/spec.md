## MODIFIED Requirements

### Requirement: Connections authenticate
A server requiring authentication SHALL say so in its Welcome and list the SASL mechanisms it accepts. The client SHALL
authenticate by an Authenticate exchange on stream 0 before its first request, and the server's final Authenticate
SHALL name the principal the connection acts for and when its credential expires. A failed exchange SHALL be refused
with code 19, saying nothing of what failed, which closes the connection. A request before the exchange completes SHALL be
refused with code 20 on the request's stream, the connection staying open for authentication, and an exchange not complete
within the authentication timeout with code 20 on stream 0, which closes the connection. Before its credential expires, a
connection SHALL be able to authenticate again on stream 0 as the same principal while its other streams go on; a
credential that expires first SHALL close the connection with code 22, aborting its open transactions and keeping its
sessions. A server running without authentication SHALL say so in its Welcome, and its connections SHALL act for the
anonymous principal without an exchange (`../../../specification/SECURITY.md`).

#### Scenario: A client authenticates with SCRAM-SHA-256
- **WHEN** a server requiring authentication lists `SCRAM-SHA-256` and a client completes the exchange with the
  principal's password
- **THEN** the server's final Authenticate names the principal, and the client's requests are served

#### Scenario: A wrong password is refused without saying why
- **WHEN** a client's SCRAM proof does not verify, once for a principal that exists and once for a name no principal
  holds
- **THEN** each is refused with code 19 on stream 0 and the connection closed, and the two Error frames are the same
  bytes

#### Scenario: A request before authentication is refused on its stream
- **WHEN** a client sends Begin after Welcome and before the final Authenticate
- **THEN** the server answers code 20 on the request's stream and the connection stays open

#### Scenario: An unfinished authentication times out
- **WHEN** a client sends Hello and then nothing for longer than the authentication timeout
- **THEN** the server answers code 20 on stream 0 and closes the connection

### Requirement: Commands keep their numbers
A command's number SHALL never be reused for another meaning; a retired command's number SHALL stay reserved; the range
256–511 SHALL be reserved for stage 8's replica commands, and the range 512–767 SHALL hold the role commands, which
clients may not send ("Roles exchange role commands").

#### Scenario: A retired number is refused
- **WHEN** a frame carries a retired command's number
- **THEN** it is refused as an unknown command

#### Scenario: A replica command number is refused in this stage
- **WHEN** a frame carries command number 300
- **THEN** it is refused as an unknown command

### Requirement: Requests name terms by their text
A request body that carries RDF terms SHALL carry them in a term table of its own, one entry per distinct term, each a
kind and offsets into the body's text section: an IRI by its text, a blank node by its label scoped to the request, a
literal by its lexical form with its datatype's IRI (none for `xsd:string`) or its language tag and base direction, and
a triple term by the indices of three earlier entries of the same table. Every term the body names SHALL be an index
into that table. A receiver SHALL refuse an entry that repeats an earlier one, a triple term naming an entry at or after
its own, and a triple term nested deeper than 32. A client names a term only by its text; it names a statement without a
statement ID by its quad, the triple term of its subject, predicate and object with its graph, and a statement with one
by its reifier's skolem IRI, as RDF 1.2 reification does; result batches name IRIs by their text through the batch's
dictionary: no system ID crosses to a client. The frontend SHALL refuse a term breaking a slot's rule with Error 7 (Refused) on the
request's stream, `detail` the term's index and the message naming the rule, a literal parameter's code 11 coming first where it
applies: a pattern's graph or subject slot takes an IRI; a pattern's object slot takes no blank node's label, which a request names
again by its skolem IRI, and no triple term; the graph or subject slot of a change or an operation takes an IRI or a blank node; and
no slot of a change or an operation takes a triple term, since no statement holds one. In a change or an operation, an IRI under a
tenant's skolem base, `https://w3id.org/ultragraphdb/ns/tenant/{n}/`, SHALL be refused the same way unless the dictionary of the partition owning the bucket its text states holds it;
in a pattern, an unheld skolem IRI SHALL match nothing, as any IRI nobody holds. The frontend SHALL compare the base exactly. One
under it naming another tenant than the request's, whose tail is not `{bucket}-{random}`, or whose bucket lies outside the tenant's
run, is an IRI nobody holds: the frontend SHALL refuse it in a change or an operation, and SHALL answer a read binding it itself
with no row, binding nothing.

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

### Requirement: Data requests, prepared statements and describe
A read SHALL be a quad pattern with each position bound or unbound, a limit and a resume key, naming its transaction,
whose Begin names the level, session token and deadline the read takes; a row SHALL name its statement by its quad and,
when the statement holds a statement ID, its reifier IRI, never by the statement ID. A write SHALL be a changeset, in
its transaction's tenant, addressing a statement's properties, or one of several parallel edges, through the
statement's reifier IRI, and a statement without one by its triple term and graph; a commit-time operation SHALL be its
own request, and its result in the Commit reply SHALL name the node it found or created by its IRI or skolem IRI. A
tenant SHALL bind per Begin, so one connection's transactions MAY act in several. A statement SHALL be prepared once per
connection, which holds it, in the tenant its Prepare names, from a quad pattern with parameter slots, answered with a
statement id, its parameter and result column types and a schema id, and executed with parameters bound by position; Execute
SHALL refuse a statement prepared in another tenant than its transaction's with code 10 (UnknownStatement), `detail` the
statement; after a new catalog version of its tenant its schema id SHALL no longer match, and an execution SHALL be refused as
stale, while recording a default entry (metadata-group, "Catalog versions take effect at an epoch boundary") SHALL keep
the schema id; a literal bound at a slot that takes a node SHALL be refused. A resume key SHALL continue its own request
only, and rows SHALL come in the store's key order. The frontend SHALL answer Describe itself from the metadata group, asking
no other role, as far as the connection's principal may read it, so it names what is declared and recorded rather than what
data holds: `read_version` the version bound of the latest epoch the sequencer closed, or of epoch 0 before the first close;
the entity types declared in effect at that epoch; the predicates of kind edge or either that it declares or holds a default
entry for as relationship types, those of kind property or either as property keys; and the catalog's declarations. An answer
not fitting one frame within the client's `max_frame` SHALL be refused with code 6 (TooLarge), `detail` that `max_frame`.

#### Scenario: A stale statement is refused
- **WHEN** a statement is prepared, the catalog of its tenant changes, and the statement is executed
- **THEN** the execution is refused as stale, and preparing it again succeeds

#### Scenario: Describe answers from the metadata group
- **WHEN** `name` is declared a property with a `term` index, `score` holds a default entry, and a client sends Describe
  for the tenant
- **THEN** the frontend answers asking no other role: `read_version` is the version bound of the latest closed epoch,
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

### Requirement: Result batches are columnar and dictionary-encoded
A Result SHALL hold `monotonic` at byte 20 and nine sections, the column names in sections 7 and 8. A Prepare SHALL hold `language`
at offset 24, seven sections and column types 1 to 13. Query (22) and Install (23) SHALL be read and built as `PROTOCOL.md` lays them
out, and codes 28 and 29 SHALL be in the error table.

A result batch SHALL hold, per column, a u32 cell per row indexing the batch's term table, `u32::MAX` for unbound; the
term table SHALL hold each distinct term of the batch once in `ultragraphdb_core_terms`' encoding, with a text section
for IRIs and for literals the store holds by hash, and with a blank node told from an IRI by its own tag, `0x02`, its
text its skolem IRI. A row SHALL name its statement by its subject, predicate, object and graph columns and, when the
statement holds a statement ID, by its reifier's skolem IRI in the reifier column, and SHALL carry no statement ID. A
literal SHALL be encoded by value only when its payload holds the value exactly. A row whose pattern binds a subject that
is a reifier SHALL name one property of the statement. A read that matches nothing SHALL
answer one batch flagged last, with no rows. Only a stream's last batch SHALL hold a resume key, and only when its read stopped
at its limit where rows may remain; a receiver SHALL refuse one on any other batch with `FieldValue` naming `resume key`, and a
term whose `text` or `annotation` is not a string index where the protocol's table names one and `ABSENT` elsewhere with
`FieldValue` naming `term text`, the term's index.

#### Scenario: A Result holds its monotonic flag and column names
- **WHEN** a Result's `monotonic` is 3, and another's sections 7 and 8 name a column count other than its `columns`
- **THEN** the receiver refuses the first with `FieldValue` naming `monotonic` and the second with `CountMismatch` naming
  `column names`

#### Scenario: A repeated term is sent once per batch
- **WHEN** a batch of 1,000 rows names the same IRI in every row
- **THEN** its term table holds that IRI once

#### Scenario: A decimal the payload cannot hold is kept by its lexical form
- **WHEN** a literal `"0.1234567890123456789"^^xsd:decimal`, of 19 fraction digits, is written and read back
- **THEN** its term carries tag `0x31`, a known datatype's code and the hash of its lexical form, and the batch's text
  holds the lexical form unchanged

#### Scenario: A literal of another datatype names its datatype by hash
- **WHEN** a Result's terms hold a literal of a datatype no code names, one of tag `0x32` and one of tag `0x33`
- **THEN** a client reads each back with its datatype by the hash of its IRI, the IRI the term's annotation, and the
  lexical form inline or as the term's text

#### Scenario: A statement with properties is named by its reifier
- **WHEN** a read returns `<a> <knows> <b>` in `<g1>`, which carries the property `<since> 2020`
- **THEN** the row names the statement by `<a>`, `<knows>`, `<b>` and `<g1>` and by its reifier's skolem IRI, and the
  batch holds no statement ID

#### Scenario: A resume key on a batch before the last is refused
- **WHEN** a Result frame not flagged `LAST` holds a resume key
- **THEN** the receiver refuses it with `FieldValue` naming `resume key`

#### Scenario: A hash-only term without its text is refused
- **WHEN** a Result's term encodes an IRI by hash, its `text` `ABSENT`
- **THEN** the receiver refuses it with `FieldValue` naming `term text`, the term's index

### Requirement: Drops are commands
A client SHALL drop a predicate, a graph, a tenant's data or everything in a tenant by a Drop request on a stream of its
own, committed through a session as a commit is or through no session, and answered with the drop's commit version and,
through a session, its session token. A receiver SHALL refuse any other `kind` with `FieldValue` naming `kind`; kind 5 stays unassigned, in
Drop and in the write-set record's operations alike. The commit step SHALL decide a drop as the last transaction of its
round, and its write-set record SHALL name the drop, so recovery redoes it. A transaction that read or wrote what a drop
removes, below the drop's version, SHALL abort with a conflict of kind Drop.

#### Scenario: A graph drop removes the graph
- **WHEN** a client drops graph `g` of a tenant holding statements in `g` and in `h`
- **THEN** the reply carries the drop's commit version, no statement of `g` remains, and every statement of `h` remains

#### Scenario: A tenant's data drop keeps its catalog
- **WHEN** a client drops a tenant's data after declaring `name` a property
- **THEN** no statement of the tenant remains, and Describe still lists `name`

#### Scenario: A drop retried through its session applies once
- **WHEN** a Drop's reply is lost, and the client sends it again with the same session and request number
- **THEN** the second answer is the stored reply, and the drop is applied once

#### Scenario: A transaction writing into a dropped graph aborts
- **WHEN** a transaction reads before a drop of graph `g` commits and then writes into `g`
- **THEN** its commit is refused with a conflict of kind Drop

#### Scenario: A Drop of kind 5 is refused
- **WHEN** a Drop's `kind` is 5, and a write-set record names operation 5
- **THEN** the Drop is refused with `FieldValue` naming `kind`, the record with `FieldValue` naming `operation`, and
  nothing is dropped

## ADDED Requirements

### Requirement: A write commits in one frame
DirectWrite SHALL carry a changeset, and ConditionalWrite commit-time operations with any changes, each under a session,
a request number, a tenant and a session token, each answered by a Commit. Each SHALL open a stream of its own and need
no Begin. Its command header SHALL hold `principal` at offset 56: 0 from a client, refused at the client endpoint with
`FieldValue` naming `principal`. The frontend SHALL relay the frame on a new stream with `principal` filled with the
connection's principal and every other command-header byte and the body unchanged, a resend the same way, and the
partition's write-set record SHALL name that principal.

#### Scenario: A DirectWrite is relayed with its principal
- **WHEN** a client authenticated as principal 9 sends a DirectWrite through the frontend
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
joining the cluster at the frontend's address and learning its members,
registering a reference at an entity's home and its reply, delivering epoch closes with the catalog versions they assign
and the session evictions they carry, a partition's request for a close, leasing a sequence block, reading the placement
map and a catalog version, and verifying a principal's proof. A role command arriving on a client connection SHALL be
refused with a protocol error naming the command, and a role connection SHALL authenticate with the cluster's role
secret (role-binaries, "Roles authenticate each other with the cluster's role secret") before its first role command. A role
endpoint's Welcome SHALL announce `max_frame` 1,048,576 and `max_streams` 4,294,967,295, and a role's Hello `max_frame` 1,048,576,
so a relayed frame takes any size a client's may and a role's stream refusal never reaches a client as a limit it did not pass;
every Welcome's `initial_credit` SHALL be at least 1. A partition SHALL answer each Write and Operation on a Bind stream with Held
(531), once it holds the request, or with an Error; the protocol's Answered-by column SHALL list Held for both on a Bind stream,
and it SHALL refuse a Write, Operation, Commit or Abort on a stream no Bind of kind 1 opened, and a Drop on one no Bind of kind 2
opened, with Error 1 naming the command. Entries SHALL name the Scan's chosen `version` in every frame, a version or version bound,
refused otherwise with `FieldValue` naming `version`, as a Scan's and a Begin's `read_version` is naming `read_version`, and `through`, the batch's
last range; a Scan's range SHALL carry `after_len` and its `after`; ComputeRead SHALL carry `max_frame` and `credit`; a Reference's
`outcome` SHALL be 0, 7, 17 or 26. A dialling side SHALL refuse an Entries frame whose `version` is not its Scan's first frame's,
with `FieldValue` naming `version`, and an entry that does not belong where its frame puts it, with `FieldValue` naming `entry order`,
the entry's index: in a run, one whose key is not the first key of a range from `range` through `through`, at or after the range of
the entry before; in a batch of one range, a key below the range's first key, or at or above its past key when it has one.

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
- **WHEN** the frontend's sequencer closes epochs 1 to 5 and storage is connected
- **THEN** storage receives five epoch-close commands, in order

#### Scenario: A Write on a Bind stream is answered by Held
- **WHEN** the frontend sends a Write on a Bind stream, its references registered and its changes held by the partition
- **THEN** the partition answers Held on the stream, which stays open; the dialling side's check accepts it, and the
  frontend relays nothing to the client

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

#### Scenario: ComputeRead carries the client's limits
- **WHEN** a ComputeRead names `max_frame` 524,287 or 1,048,577, or `credit` 0
- **THEN** it is refused with `FieldValue` naming `max_frame` or `credit`

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
A partition SHALL copy a request waiting on a home out of its connection's receive buffer, go on reading that connection, and send
the request's answer on its stream once it has one, as `PROTOCOL.md` section 11.12's *Parked requests* states. A stream's requests
SHALL take effect and be answered in the order they were sent. A connection's parked bytes SHALL be bounded by `storage.held_bytes`,
at which the partition stops reading that connection until they fall below it, refusing nothing.

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

### Requirement: A transaction names its reads' level, session token and deadline
Begin SHALL name the level of the transaction's reads (latest, snapshot or monotonic-fast), the session token its client
holds, if any, its deadline, and the read version the client names, if any, which the partition answering checks against
its watermark (transactions, "Transactions read a snapshot at the visibility watermark"), all of which the transaction's
reads share; a read request SHALL name its transaction and no level of its own (quad-reads, "A read's transaction names
its level"). A read-write transaction SHALL read its partition's latest state whatever the level, and its token SHALL
make its commit carry an epoch at or above the token's. A read-only transaction carrying a token naming an epoch above
the latest closed one SHALL make its reads wait until that epoch closes, at most until the deadline, and SHALL then
refuse them as timed out. A commit reply SHALL carry the commit's session token, zero when `session` is all zero. A Begin, and
each of its transaction's requests, SHALL act in the tenant it names.

#### Scenario: A read with a future token waits
- **WHEN** a read-only transaction begins with a token of epoch 30 while epoch 29 is the latest closed, reads, and
  epoch 30 closes
- **THEN** the read answers after epoch 30 closes, at that epoch or later

#### Scenario: A read past its transaction's deadline is refused
- **WHEN** a read-only transaction begins with a token of epoch 30 and a deadline of 50 ms, and epoch 30 does not close
  within it
- **THEN** its read is refused as timed out

### Requirement: The client's credit is automatic
The client shells SHALL grant credit on a read's stream automatically as its batches are consumed and SHALL expose no
manual grant; the core SHALL grant no credit of zero frames, refused on the wire.

#### Scenario: A long read flows without a manual grant
- **WHEN** a client reads 10,000 rows through a shell with an initial credit of 4 frames
- **THEN** every row arrives, credit being granted as batches are consumed, and no grant of zero frames is sent

### Requirement: A frame past its stream's credit is refused
Every receiver of Result or Entries frames, always the side that dialled, SHALL refuse a frame past its stream's credit as a
protocol violation, code 1 (Protocol) by the rule `PastCredit` naming the stream, and SHALL close the connection, sending no
Error, which only the side that accepted sends.

#### Scenario: A Result frame past its credit closes the client's connection
- **WHEN** a server sends a read's stream a Result frame with the stream's credit spent
- **THEN** the client refuses it as `PastCredit`, naming the stream, and closes the connection

#### Scenario: A Result frame past its credit closes the frontend's role connection
- **WHEN** a compute process sends a ComputeRead's stream a Result frame with the stream's credit spent, the frames its credit and a Credit allowed having reached the client and a second read being open on the connection
- **THEN** the frontend closes the role connection, answers the first read code 17, and sends the second to the other compute process

#### Scenario: An Entries frame past its credit closes compute's role connection
- **WHEN** a partition sends a Scan's stream an Entries frame with the stream's credit spent
- **THEN** compute refuses it as `PastCredit`, naming the stream, and closes the role connection

### Requirement: The Rust client retries a busy Register
The Rust client SHALL send a Register answered code 14 again itself, on a new stream after its retry delay and within its commit
attempts, and SHALL then return that code 14. Both shells SHALL behave alike.

#### Scenario: A Register answered busy is sent again
- **WHEN** a client of 4 commit attempts registers, and a full registry answers its first two Registers code 14
- **THEN** the client sends the Register three times, each on a new stream, and returns the session

#### Scenario: A Register busy past its attempts returns code 14
- **WHEN** a client of 3 commit attempts registers, and a full registry answers every Register code 14
- **THEN** the client sends the Register three times and returns code 14, and the connection stays open

#### Scenario: A connection lost under a Register's wait ends it
- **WHEN** a Register answered code 14 waits for its next send, and the connection is lost
- **THEN** the client returns the lost connection, and sends the Register on no later connection

### Requirement: A commit in flight is in doubt once sent
When a connection is lost or closed with a commit in flight, the client SHALL answer `ConnectionLost` if the commit's
frame was never sent on any connection, and `InDoubt` if it was sent, on any attempt; an exhausted retry SHALL answer
`InDoubt`, and a Commit, Drop or one-frame write answered code 17 SHALL be in doubt the same way. The session SHALL resolve an
in-doubt commit by resending it with the same request number, which the partitions' session tables answer (transactions,
"Exactly-once commits through sessions"): the stored reply says it committed and code 18 that it did not; code 14 says it is
undecided and code 17 that a partition could not be asked, so it stays in doubt and the client resends after a delay; codes 12
and 13 leave it unknown for good, and after 13 the counter moves above `detail`. A resent one-frame write or Drop has no code 18:
a Commit or Drop reply says it committed, codes 12, 13, 14 and 17 read as above, codes 21 and 23, which the frontend gives
before any partition's table decides, leave it in doubt, and any other refusal says it did not. A commit
through no session, a declaration's among them, SHALL have no resend: a lost reply or code 17 reports its outcome as unknown.

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
The frontend SHALL refuse a Write, Operation or Abort on a new stream, and a Commit through no session on a new stream,
with Error 1 (Protocol) on that stream, the message naming the command; and a Write, Operation or Commit on a read-only
transaction's stream the same way, ending the transaction. A frame on a stream number at or below the highest the
connection opened, on a stream no longer open, is late, and a server SHALL drop it, a Commit and a Drop included. A
Write, Operation or Commit on a transaction's stream after its Commit is late, and the frontend SHALL drop it, the Commit
deciding the transaction. A read naming a transaction whose Error has not yet left SHALL be answered with that Error, as the
transaction's reads still open are. A dialling side SHALL take a frame on a stream it has ended, numbered below the next stream it will open, through the
frame's checks and then drop it, whatever its command, and SHALL refuse a stream at or above that number as answering no
request.

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
- **WHEN** a client sends a Write and a second Commit on its transaction's stream after the Commit the frontend relayed,
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

### Requirement: A client takes a refusal after rows as the read's failure
Only a stream ending in `LAST` SHALL be an answer. A client SHALL take a refusal after rows as the read's failure: the
stream has ended, so it sends no Credit or Cancel for it, and no row it carried counts. The refusal SHALL end the read
alone unless the role connections' rules end the transaction with it. The Rust client SHALL return the refusal after the
batches already handed over and resend nothing of a read; running it again is its caller's choice. What a retryable code
runs again SHALL be the code's own: the same request on a new stream for code 14, and code 5 on a Drop or one-frame
write; the whole transaction on a new Begin for codes 2, 3, 4, 18, 26, and 5 on a Commit; the read for code 24, and for code
17 on a read, from its start or a resume key, its earlier rows discarded.

#### Scenario: A refusal after rows fails the read
- **WHEN** a read delivers two batches and is then refused with code 24
- **THEN** the client returns the two batches and then the refusal, sends no Credit or Cancel on the stream, and the
  transaction stays open

#### Scenario: A code's retry names what runs again
- **WHEN** a commit is refused with code 2, and a read with code 24
- **THEN** the client's error says of the first that the whole transaction runs again on a new Begin, and of the second
  that the read runs again

#### Scenario: Code 17 runs a read again and leaves a commit in doubt
- **WHEN** a read is refused with code 17, and a Commit with code 17
- **THEN** the client's error says of the first that the read runs again, and of the second that it is in doubt

### Requirement: The client's timeout leaves a read its deadline
The Rust client's timeout for a read SHALL run from the later of its last progress and the Begin's deadline, SHALL be at
least 30 s, the most `scan_stall_ms` holds, and SHALL not run for a stream whose credit is spent, since a read's silence
within its transaction's deadline is a wait for a close and a hung compute process is answered with code 17 by the
deadline plus `scan_stall_ms`.

#### Scenario: A long deadline survives the client's timeout
- **WHEN** a read-only transaction's deadline is 40 s, and its read waits 35 s for a close, the client's timeout set at
  30 s
- **THEN** the connection stays open, and the read answers once the close arrives

#### Scenario: A stream without credit runs no timeout
- **WHEN** a read's stream has spent its credit, and its caller takes no batch for longer than the timeout
- **THEN** the connection stays open

### Requirement: An OperationFailed Error carries the value found within the floor of `max_frame`
An Error of code 8 (OperationFailed) SHALL be built within 524,288 bytes, the floor of `max_frame`, its value found taking
what that frame leaves beside an empty message and the message what the value leaves. A literal's lexical form past it
SHALL be cut at its last character boundary within it, its datatype's IRI or language tag and direction whole; an IRI or a
blank node past it, which a cut would make name another node, SHALL be left out, the term table empty. Either SHALL set bit
0 of `detail`, and a value carried whole SHALL leave it clear, so an empty table with the bit clear still means no value
held. Every other bit of code 8's `detail` SHALL be 0: a receiver SHALL refuse an Error of code 8 with any other bit set,
with `FieldValue` naming `detail`, as one of code 26 with any of bits 24 to 31 set, and a sender SHALL build none. The Rust
client SHALL say a refusal's value was cut, and the caller reads the value again for it whole.

#### Scenario: A value found past the frame arrives cut and flagged
- **WHEN** a ConditionalWrite's compare-and-set fails on a literal of 524,288 bytes, and another on a literal as long as
  the first refusal carried
- **THEN** the first refusal holds the literal's start, cut, and the client says it was cut; the second holds the
  literal whole, and the client says nothing was cut

#### Scenario: An OperationFailed Error sets no bit of `detail` but bit 0
- **WHEN** an Error of code 8 arrives with `detail` 1, another with bit 1 set, and another with bit 63 set
- **THEN** the receiver accepts the first, its value cut, and refuses the others with `FieldValue` naming `detail`; a
  sender builds neither
