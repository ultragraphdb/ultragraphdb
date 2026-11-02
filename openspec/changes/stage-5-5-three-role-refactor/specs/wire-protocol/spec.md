## MODIFIED Requirements

### Requirement: Requests name terms by their text
A request body that carries RDF terms SHALL carry them in a term table of its own, one entry per distinct term, each a
kind and offsets into the body's text section: an IRI by its text, a blank node by its label scoped to the request, a
literal by its lexical form with its datatype's IRI (none for `xsd:string`) or its language tag and base direction, and
a triple term by the indices of three earlier entries of the same table. Every term the body names SHALL be an index
into that table. A receiver SHALL refuse an entry that repeats an earlier one, a triple term naming an entry at or
after its own, and a triple term nested deeper than 32. A client names a term only by its text, and result batches
name IRIs by their text through the batch's dictionary: no system ID crosses to a client.

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

## ADDED Requirements

### Requirement: Roles exchange role commands
Roles SHALL exchange role commands in the same frames as clients, in a command range clients may not send: routing a
write or a read to a partition, delivering epoch closes, leasing a sequence block, reading the placement map and a
catalog version, and verifying a principal's proof. A role command arriving on a client connection SHALL be refused
with a protocol error naming the command, and a role connection SHALL authenticate with the cluster's role secret
(role-binaries, "Roles authenticate each other with the cluster's role secret") before its first role command.

#### Scenario: A client may not send a role command
- **WHEN** a client connection sends a block-lease command
- **THEN** the frame is refused with a protocol error naming the command, and nothing is leased

#### Scenario: Epoch closes reach storage in order
- **WHEN** the frontend's sequencer closes epochs 1 to 5 and storage is connected
- **THEN** storage receives five epoch-close commands, in order

### Requirement: Reads carry their level and session token
A read request SHALL name its level (latest, snapshot or monotonic-fast) and SHALL carry the session token its client
holds, if any; a commit reply SHALL carry the commit's session token. A token naming an epoch above the latest closed one SHALL make the read wait
until that epoch closes, at most the request's deadline, and SHALL then be refused as timed out.

#### Scenario: A read with a future token waits
- **WHEN** a read carries a token of epoch 30 while epoch 29 is the latest closed, and epoch 30 closes
- **THEN** the read answers after epoch 30 closes, at that epoch or later

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
