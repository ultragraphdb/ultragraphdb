## MODIFIED Requirements

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
entry for as relationship types, those of kind property or either as property keys; and the catalog's declarations: for a governed tenant the
records of kinds 8, 9 and 10 derived from its Predicate, Type and Role entries, in sections 5 and 6, and in sections 7
and 8 the catalog version's entries by the kinds of `PROTOCOL.md` section 8.3, in the version's order, the installed
sources among them. An answer not fitting one frame within the client's `max_frame` SHALL travel in
continuation frames, the last flagged `LAST`.

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

#### Scenario: A Describe past the client's frame continues
- **WHEN** a tenant's Describe answer does not fit one frame within the client's `max_frame`
- **THEN** it arrives in continuation frames whose sections concatenate to the whole answer, the last flagged `LAST`

#### Scenario: A statement prepared in another tenant is refused
- **WHEN** a connection prepares a statement in tenant 1 and executes it in a transaction of tenant 0
- **THEN** the execution is refused with code 10, `detail` the statement, and the transaction goes on

#### Scenario: One connection's transactions act in two tenants
- **WHEN** a connection begins one transaction in tenant 0 and another in tenant 1, and each writes and reads `<a> <p>
  <b>`
- **THEN** each reads its own statement, and neither reads the other's

### Requirement: Result batches are columnar and dictionary-encoded
A Result SHALL hold `monotonic` at byte 20 and twelve sections, the column names in sections 7 and 8 and the warnings in 9
to 11. A Prepare SHALL hold `language` at offset 24, ten sections, the warnings in 7 to 9, and column types 1 to 13. Query (22) and Install (23) SHALL be read and built as `PROTOCOL.md` lays them
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
`outcome` SHALL be 0, 7, 17, 26 or 30, and 0, 17 or 26 in get-only mode. A dialling side SHALL refuse an Entries frame whose `version` is not its Scan's first frame's,
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

## ADDED Requirements

### Requirement: The frames follow PROTOCOL.md as extended for UGS
`ultragraphdb_protocol_wire` SHALL read and build, as `PROTOCOL.md` lays them out: the warnings structure of section
4.9 in Result's first batch and in Prepare's and Install's replies; Describe's entry kinds 1 to 20 for the catalog entries, the sources and the retired names, its request of nine
sections and its continuation frames (section 8.3); Install with `staged`, the migration report's
section and the warnings' sections, and a source's text (section 8.4); Reference's `mode` and `status`, a retracted participant's among them (section 13.15); Drop's `catalog` (section
6.5); the reifier column's terms, a skolem IRI of tag `0x02` or a declared relation's handle IRI of tag `0x01`
(sections 9.1, 10.2);
the error codes 30 (EntityDeleted) and 31 (Fenced); the datatypes 34 to 36 with their tags and by-value payloads
(sections 10.1, 10.4, 10.5); Catalog's continuation frames and the limit `catalog_bytes_max` (sections 11.9, 13.7); the write-set record's
operations 8 and 9 (section 12.2); and
Purge (24) and Migrate (25), whose frames decode and are refused with code 7 naming their stage. Every decoder SHALL
refuse a malformed body with the refusal its section's order names and SHALL never panic. Each golden frame of Result,
Prepare, Describe, Install and Reference SHALL be regenerated by its generator. Describe's and Catalog's continuation frames SHALL spend credit as
Result frames do: the side that dialled SHALL grant it as frames are consumed and SHALL refuse a frame past its
stream's credit as `PastCredit`.

#### Scenario: An Install reply carries its warnings
- **WHEN** an Install reply with one warning `UGS0902` is built and read back
- **THEN** the view answers the version, the epoch, the report and one warning whose `diagnostic` is 902 in language
  4, with its source, location and message

#### Scenario: A request with a report is refused
- **WHEN** an Install request's section 2 is not empty
- **THEN** it is refused with `CountMismatch` naming `report`

#### Scenario: Purge and Migrate decode and are refused by stage
- **WHEN** a client sends a well-formed Purge and a well-formed Migrate
- **THEN** each is refused with code 7 on its stream, the message naming the command and its stage, and the connection
  stays open

#### Scenario: A Drop of a predicate names no catalog version
- **WHEN** a Drop of kind 1 carries `catalog` 7
- **THEN** it is refused with `FieldValue` naming `catalog`

#### Scenario: A handle IRI is taken in the reifier column
- **WHEN** a pattern's Result batch holds, in its reifier column, a term of tag `0x01` and another of tag `0x30`
- **THEN** the first batch is read, and the second is refused with `FieldValue` naming `reifier`

#### Scenario: A Describe of five frames takes a Credit
- **WHEN** a Describe answer takes five frames and the Welcome's `initial_credit` is 4
- **THEN** the fifth frame is sent only after the client's Credit, and a fifth frame sent without one is refused as
  `PastCredit`

#### Scenario: The golden frames match their generators
- **WHEN** the conformance suite reads every golden frame of Result, Prepare, Describe, Install and Reference
- **THEN** each decodes, and building it again from its view gives the same bytes

### Requirement: Query carries UQL as language 3
Query (22) SHALL take `language` 1, GQL, 2, SPARQL, or 3, UQL, and Prepare (10) 0, a quad pattern, or 1 to 3; any other
value SHALL be refused with `FieldValue` naming `language`. A refusal of a UQL text SHALL be code 29 with the diagnostic's number
in `detail` and 3 in its bits 16 to 23.

#### Scenario: Language 3 is read and built
- **WHEN** a Query with `language` 3, the text `Company { id }` and no parameters is built and read back
- **THEN** the view answers language 3 and the text

#### Scenario: Language 4 is refused
- **WHEN** a Query names `language` 4
- **THEN** it is refused with `FieldValue` naming `language`

### Requirement: Operation deletes and retracts an instance
Operation (9) SHALL take kind 5, delete an instance, and kind 6, retract an instance, each with two args, the graph, a
term or `ABSENT`, and the instance's IRI, and with `delta`, `properties` and `links` zero. Its result in the Commit
reply SHALL hold `node` `ABSENT`, `value` 0 and `created` 0, as a compare-and-set's does. A ConditionalWrite's
operations SHALL take both kinds. A kind above 6 SHALL be refused with `FieldValue`
naming `kind`.

#### Scenario: A retract operation is read and built
- **WHEN** an Operation of kind 6 naming the default graph and `https://example.org/deep_brain#Node/ORG%3Aacme` is
  built and read back
- **THEN** the view answers kind 6, the graph `ABSENT` and the IRI

#### Scenario: A delete operation with a third arg is refused
- **WHEN** an Operation of kind 5 carries three args
- **THEN** it is refused with `CountMismatch` naming `args`
