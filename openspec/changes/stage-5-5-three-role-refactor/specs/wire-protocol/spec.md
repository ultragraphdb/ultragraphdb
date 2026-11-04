## MODIFIED Requirements

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

### Requirement: A write commits in one frame
WriteCommit SHALL carry a changeset and commit-time operations under a session, a request number and a session token, and SHALL be
answered by a Commit. It SHALL open a stream of its own and need no Begin, and the frontend SHALL relay it unchanged.

#### Scenario: A WriteCommit is relayed unchanged
- **WHEN** a client sends a WriteCommit through the frontend
- **THEN** the frame the partition receives holds the client's command header and body byte for byte, and the client receives a
  Commit

#### Scenario: A malformed WriteCommit is refused
- **WHEN** a WriteCommit holds neither changes nor operations
- **THEN** it is refused with `CountMismatch` naming `changes`

### Requirement: Requests name terms by their text
A request body that carries RDF terms SHALL carry them in a term table of its own, one entry per distinct term, each a
kind and offsets into the body's text section: an IRI by its text, a blank node by its label scoped to the request, a
literal by its lexical form with its datatype's IRI (none for `xsd:string`) or its language tag and base direction, and
a triple term by the indices of three earlier entries of the same table. Every term the body names SHALL be an index
into that table. A receiver SHALL refuse an entry that repeats an earlier one, a triple term naming an entry at or after
its own, and a triple term nested deeper than 32. A client names a term only by its text; it names a statement without a
statement ID by its quad, the triple term of its subject, predicate and object with its graph, and a statement with one
by its reifier's skolem IRI, as RDF 1.2 reification does; result batches name IRIs by their text through the batch's
dictionary: no system ID crosses to a client.

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

### Requirement: Data requests, prepared statements and describe
A read SHALL be a quad pattern with each position bound or unbound, a limit and a resume key, naming its transaction,
whose Begin names the level, session token and deadline the read takes; a row SHALL name its statement by its quad and,
when the statement holds a statement ID, its reifier IRI, never by the statement ID. A write SHALL be a changeset, in
its transaction's namespace, addressing a statement's properties, or one of several parallel edges, through the
statement's reifier IRI, and a statement without one by its triple term and graph; a commit-time operation SHALL be its
own request, and its result in the Commit reply SHALL name the node it found or created by its IRI or skolem IRI. A
statement SHALL be prepared once per connection, which holds it, from a quad pattern with parameter slots, answered with
a statement id, its parameter and result column types and a schema id, and executed with parameters bound by position;
after a new catalog version of its namespace its schema id SHALL no longer match, and an execution SHALL be refused as
stale, while recording a default entry (metadata-group, "Catalog versions take effect at an epoch boundary") SHALL keep
the schema id; a literal bound at a slot that takes a node SHALL be refused. A resume key SHALL continue its own request
only, and rows SHALL come in the store's key order. Describe SHALL answer from the data dictionary, as far as the
connection's principal may read it.

#### Scenario: A stale statement is refused
- **WHEN** a statement is prepared, the catalog of its namespace changes, and the statement is executed
- **THEN** the execution is refused as stale, and preparing it again succeeds

#### Scenario: Describe answers from the data dictionary
- **WHEN** `name` is declared a property with a `term` index and a client sends Describe for the namespace
- **THEN** the answer lists `name` as a property with its `term` index, as the data dictionary reports it

#### Scenario: A literal at a node slot is refused
- **WHEN** a statement's subject parameter is bound to a literal
- **THEN** the execution is refused with code 11, `detail` the statement

#### Scenario: A commit-time operation's node is named by its IRI
- **WHEN** a transaction's insert-if-absent finds the node `http://ex.org/x` holding the value, and the transaction
  commits
- **THEN** the Commit reply names `http://ex.org/x` by its text, and holds no system ID

### Requirement: Result batches are columnar and dictionary-encoded
A result batch SHALL hold, per column, a u32 cell per row indexing the batch's term table, `u32::MAX` for unbound; the
term table SHALL hold each distinct term of the batch once in `ultragraphdb_core_terms`' encoding, with a text section
for IRIs and for literals the store holds by hash, and with a blank node told from an IRI by its own tag, `0x02`, its
text its skolem IRI. A row SHALL name its statement by its subject, predicate, object and graph columns and, when the
statement holds a statement ID, by its reifier's skolem IRI in the reifier column, and SHALL carry no statement ID. A
literal SHALL be encoded by value only when its payload holds the value exactly. A read that matches nothing SHALL
answer one batch flagged last, with no rows.

#### Scenario: A repeated term is sent once per batch
- **WHEN** a batch of 1,000 rows names the same IRI in every row
- **THEN** its term table holds that IRI once

#### Scenario: A decimal the payload cannot hold is kept by its lexical form
- **WHEN** a literal `"0.1234567890123456789"^^xsd:decimal`, of 19 fraction digits, is written and read back
- **THEN** its term carries tag `0x31`, a known datatype's code and the hash of its lexical form, and the batch's text
  holds the lexical form unchanged

#### Scenario: A statement with properties is named by its reifier
- **WHEN** a read returns `<a> <knows> <b>` in `<g1>`, which carries the property `<since> 2020`
- **THEN** the row names the statement by `<a>`, `<knows>`, `<b>` and `<g1>` and by its reifier's skolem IRI, and the
  batch holds no statement ID

## ADDED Requirements

### Requirement: Roles exchange role commands
Roles SHALL exchange role commands in the same frames as clients, in a command range clients may not send: routing a
write or a read to a partition, a reverse read's request to the partitions an entity's referrer set names included,
joining the cluster at the frontend's address and learning its members,
registering a reference at an entity's home and its reply, delivering epoch closes with the catalog versions they assign
and the session evictions they carry, a partition's request for a close, leasing a sequence block, reading the placement
map and a catalog version, and verifying a principal's proof. A role command arriving on a client connection SHALL be
refused with a protocol error naming the command, and a role connection SHALL authenticate with the cluster's role
secret (role-binaries, "Roles authenticate each other with the cluster's role secret") before its first role command.

#### Scenario: A client may not send a role command
- **WHEN** a client connection sends a block-lease command
- **THEN** the frame is refused with a protocol error naming the command, and nothing is leased

#### Scenario: A registration is a role command
- **WHEN** a partition of one storage process registers a reference to an entity homed in another storage process
- **THEN** it sends a registration role command to that process, and receives the entity's ID, the member's generation
  and the home's latest delivered close in the reply

#### Scenario: Epoch closes reach storage in order
- **WHEN** the frontend's sequencer closes epochs 1 to 5 and storage is connected
- **THEN** storage receives five epoch-close commands, in order

### Requirement: A transaction names its reads' level, session token and deadline
Begin SHALL name the level of the transaction's reads (latest, snapshot or monotonic-fast), the session token its client
holds, if any, its deadline, and the read version the client names, if any, which the partition answering checks against
its watermark (transactions, "Transactions read a snapshot at the visibility watermark"), all of which the transaction's
reads share; a read request SHALL name its transaction and no level of its own (quad-reads, "A read's transaction names
its level"). A read-write transaction SHALL read its partition's latest state whatever the level, and its token SHALL
make its commit carry an epoch at or above the token's. A read-only transaction carrying a token naming an epoch above
the latest closed one SHALL make its reads wait until that epoch closes, at most until the deadline, and SHALL then
refuse them as timed out. A commit reply SHALL carry the commit's session token.

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

### Requirement: A commit in flight is in doubt once sent
When a connection is lost or closed with a commit in flight, the client SHALL answer `ConnectionLost` if the commit's
frame was never sent on any connection, and `InDoubt` if it was sent, on any attempt; an exhausted retry SHALL answer
`InDoubt`, and the session SHALL resolve an in-doubt commit by resending it with the same request number, which the
partitions' session tables answer (transactions, "Exactly-once commits through sessions").

#### Scenario: An unsent commit is lost, a sent one in doubt
- **WHEN** the connection closes once with a commit queued and unsent, and once with a commit sent and unanswered
- **THEN** the first answers `ConnectionLost`, the second `InDoubt`, and resolving the second reports whether it committed
