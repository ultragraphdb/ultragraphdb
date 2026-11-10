## MODIFIED Requirements

### Requirement: Handshake, streams, credit and cancellation
A client SHALL be configured with the cluster identity it connects to. A connection SHALL begin with Hello, carrying
that identity in its header and the client's protocol range, release and largest frame, answered by Welcome with the
chosen protocol, whether the connection authenticates and by which mechanisms, the maximum frame, the streams allowed in
flight, the initial credit, the largest write set, a transaction's time limit and `value_bytes_max`, the longest value a
write may hold, measured as `PROTOCOL.md` section 5.2 states: the store's `tile_bytes_max / 4`, 4,096 to 262,144, a
Welcome naming another refused with `FieldValue` naming `value_bytes_max`. Each request SHALL take a stream id,
and the server SHALL answer streams independently, in whatever order their results are ready. Every reply that ends its
stream SHALL carry the flag last, and Begin's reply SHALL not: a transaction's stream stays open until its Commit's
reply, its Abort or an error. A transaction's reads SHALL each take a stream of their own, naming the transaction, and
run beside each other at its snapshot; when the transaction ends, its reads still open SHALL end with it. A result
stream SHALL send frames only while the client holds credit, which `Credit` frames grant, starting with the initial
credit for each read. `Cancel` SHALL end a stream, after which no further frame of it is sent: on a read's stream it
ends the read alone, on a transaction's stream the transaction.

#### Scenario: Credit stops a stream
- **WHEN** a client grants credit for two frames of a result of ten
- **THEN** the server sends two frames and waits until more credit arrives

#### Scenario: Cancel ends a stream
- **WHEN** a client cancels a stream mid-result
- **THEN** no further frame of that stream arrives, and other streams continue

#### Scenario: A transaction's reads run on streams of their own
- **WHEN** a transaction runs two reads at once, each on a stream of its own naming the transaction
- **THEN** both stream their results at the transaction's snapshot, each with its own credit, and the transaction then
  commits on its own stream

#### Scenario: Cancelling a read keeps its transaction
- **WHEN** a client cancels one of its transaction's reads mid-result and then commits the transaction
- **THEN** no further frame of the read arrives, and the commit succeeds

#### Scenario: A transaction's end ends its reads
- **WHEN** a transaction commits while one of its reads has frames left to send
- **THEN** the read's stream sends no further frame, and the commit's reply arrives

#### Scenario: A Welcome announces the longest value
- **WHEN** a storage cluster runs with `tile_bytes_max` 65,536, and a Welcome naming `value_bytes_max` 4,095 arrives
- **THEN** the frontend's Welcome names `value_bytes_max` 16,384, and the one of 4,095 is refused with `FieldValue`
  naming `value_bytes_max`

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
only, and rows SHALL come partition by partition, in ascending order of the partitions the read asks as the placement
map in effect at its epoch names them, and within a partition in its families' order (`PROTOCOL.md` section 9.1). The frontend SHALL answer Describe itself from the metadata group, asking
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

#### Scenario: Rows come partition by partition
- **WHEN** a read with its subject unbound matches rows in partitions 1 and 2
- **THEN** every row of partition 1 arrives before any row of partition 2, whatever their terms

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
opened, with Error 1 naming the command. A Scan SHALL carry its `tenant` in its command header and, per 64-byte range, its
partition, `space`, `kind`, `tokenizer`, the offset and length of its key or resume key, its `graph` and its `predicate`, as
`PROTOCOL.md` section 13.13 lays them out and refused in that section's order, a range of space 1 naming one record by its
logical key and a range of space 2 a family read whole, neither naming a slot. Entries SHALL name the Scan's chosen `version` in
every frame, a version or version bound, refused otherwise with `FieldValue` naming `version`, as a Scan's and a Begin's
`read_version` is naming `read_version`, `through`, the batch's last range, `end` 0, 1 or 2, `widths`, the family's inline
columns' widths, and, for a range of space 2 whose frame has `end` 0 or 2, the resume key in its section 1 (section 13.14);
ComputeRead SHALL carry `max_frame` and `credit`; a Reference's `outcome` SHALL be 0, 7, 17 or 26. A dialling side SHALL refuse an
Entries frame whose `version` is not its Scan's first frame's, with `FieldValue` naming `version`, or whose `widths` are not the
first frame's of the same range, with `FieldValue` naming `widths`, and an entry that does not belong where its frame puts it,
with `FieldValue` naming `entry order`, the entry's index: in a run, one whose key is not the key of a range from `range` through
`through`, after the range of the entry before; in a batch of a range of space 1, a key other than the range's; in a batch of a
range of space 2, a key not beginning with the range's graph, when it names one.

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

#### Scenario: A row larger than a frame continues across frames
- **WHEN** a row's object records span two Entries frames, the first with `continues` 1
- **THEN** the second's first entry holds the same key, and the records of both are the row's, in order

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

#### Scenario: A Scan's range bytes are read from the frame
- **WHEN** a Scan's first range, of space 1, holds `key_len` 16, and its second, of space 2, a `resume_len` above 0
- **THEN** the view reads the first range's key and the second's resume key borrowed from the frame, the second's
  starting where the first's ends

#### Scenario: A range's fields are checked against its space
- **WHEN** a Scan holds a range of space 1 and kind 6, one of space 2 and kind 4 with `tokenizer` 0, one of space 1
  with a `resume_len` above 0, and one of space 2 whose `predicate` is 16 zero bytes
- **THEN** each is refused, with `FieldValue` naming `kind`, `tokenizer`, `range` and `predicate`, before any frame

#### Scenario: A resume key rides only on a range that stopped
- **WHEN** an Entries frame of a range of space 2 holds `end` 1 and a resume key, and another holds `end` 2 and none
- **THEN** each is refused with `CountMismatch` naming `resume key`

#### Scenario: Widths stay within a range
- **WHEN** a range's second Entries frame names other `widths` than its first
- **THEN** the receiver refuses it with `FieldValue` naming `widths`, and closes the connection

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
- **WHEN** a run's entry holds a key that is the key of none of its ranges, or a range of space 1's entry holds a key
  other than the range's
- **THEN** the receiver refuses it with `FieldValue` naming `entry order`, the entry's index, and closes the connection

#### Scenario: A Reference answers a home's failed store
- **WHEN** a home's store fails under a registration
- **THEN** the reply's `outcome` is 17, its `generation` and `close` 0, and a receiver accepts it

## ADDED Requirements

### Requirement: Frontier and Neighbours carry storage's second read kernel
Frontier (532) and Neighbours (533) SHALL be role commands, read and built as `PROTOCOL.md` sections 13.21 and 13.22 lay
them out and refused in those sections' order, before any frame. A Frontier SHALL carry its `lane`, 1 L, 2 S or 3 X; its
`kind`, 1 to 6, and for kind 4 its `tokenizer`; its `filters`, 0 to 8; its partition, tenant, `read_version`, principal
and, in lane X alone, `transaction`; a neighbour type range for kind 1; `top`; its `graph`, a valid system ID, 16 zero
bytes for the default graph or 16 bytes of `0xFF` for every graph the partition holds the family in; its `predicate`, a
valid system ID or 16 bytes of `0xFF` for every predicate of the kind; its rows' logical keys, each at least one byte and none
equal to an earlier row's; its 48-byte filters; and its resume key. A Neighbours frame SHALL carry its rows and entries
in the request's order, each entry's neighbour, its bytes beginning with its graph's and then its predicate's system ID
when the request names every graph or every predicate, its statement and `valid` bits, the inline values, the `version` it read,
`end`, `widths`, and its resume key exactly when `end` is 0. Neighbours frames SHALL spend credit as Entries frames do,
and a dialling side SHALL refuse one past its stream's credit as `PastCredit` and close the connection. A dialling side
SHALL refuse, closing the connection: in lanes S and X a `version` other than the first frame's; `widths` other than the
first frame's; a frame after one with `end` 1; a row continued by a frame whose first row is not that row, and a frame's
first row not above the previous frame's last when that one did not continue; `LAST` on a frame without `end` 1, or
`end` 1 without `LAST`.

#### Scenario: A row named twice is refused
- **WHEN** a Frontier of kind 1 names one subject's system ID as its first and its third row
- **THEN** it is refused with `FieldValue` naming `row`, the third row, and no frame precedes the refusal

#### Scenario: A Frontier's fields are checked
- **WHEN** a Frontier of lane 3 names no transaction, one of kind 6 holds a filter, one of kind 3 names a type range,
  and one holds a filter whose `bounds` sets bit 2
- **THEN** each is refused, with `FieldValue` naming `transaction`, `filters`, `prefix` and `filter`

#### Scenario: A lane L answer may name a version per frame
- **WHEN** a lane L Frontier's answer takes two Neighbours frames naming two versions, and a lane S Frontier's does
  the same
- **THEN** the receiver reads the first answer, and refuses the second's second frame with `FieldValue` naming
  `version`, closing the connection

#### Scenario: An every-graph answer names each entry's graph
- **WHEN** a Frontier of kind 1 names every graph and every predicate, and its Neighbours frame is read
- **THEN** each entry's neighbour begins with its graph's system ID, then its predicate's, then the object's

#### Scenario: A resume key rides only on a frame the answer continues past
- **WHEN** a Neighbours frame holds `end` 0 and no resume key, and another `end` 1 and a resume key
- **THEN** each is refused with `CountMismatch` naming `resume key`

#### Scenario: A Neighbours frame past its credit closes compute's role connection
- **WHEN** a partition sends a Frontier's stream a Neighbours frame with the stream's credit spent
- **THEN** compute refuses it as `PastCredit`, naming the stream, and closes the role connection

### Requirement: Ingest inserts blocks in one frame
Ingest (26) SHALL be read and built as `PROTOCOL.md` section 6.8 lays it out and refused in that section's order: its
command header of `session`, `request`, `tenant`, `token`, `principal`, `catalog` and `share`; a request holding the term
table, at least one 32-byte block of kind 1, a relation block, or 2, an entity block, and the blocks' data, each part
8-aligned, and no outcome; a reply holding the request's command header, sections 0 to 3 empty, and one 48-byte outcome
per share, by `share` from 1, the partitions ascending. What an Ingest does, the checks of its fields at the client
endpoint, its stream, its shares and their outcomes, is the ingest capability's ("Ingest is a one-frame insert under
DirectWrite's rules", "An Ingest frame is checked in its stated order").

#### Scenario: An Ingest round-trips
- **WHEN** an Ingest holding one relation block of 2 rows and 3 neighbours with one column, and one entity block of 2
  rows with two columns, one of them variable-width, is built and read back
- **THEN** the view answers each block's rows, neighbours, columns, validity bits and values as built

#### Scenario: A block of no rows is refused
- **WHEN** an Ingest holds a block of `rows` 0
- **THEN** it is refused with `CountMismatch` naming `block rows`

#### Scenario: A committed outcome names no failing block
- **WHEN** an Ingest reply's outcome holds `outcome` 0 with `block` 0
- **THEN** the receiver refuses it with `FieldValue` naming `outcome`, the outcome's index

### Requirement: Codes 5 and 6 carry the store's refusals
Error code 5 (Busy) SHALL carry `detail` 0, or the store's admission refusal of a commit: 1 when its delta stays at its
watermark, 2 when its grid holds too few free extents (`GridFull`), both retryable as code 5 is (`PROTOCOL.md` section
9.4; `STORAGE.md` sections 12.4 and 15.6). Error code 6 (TooLarge) SHALL refuse a write holding a value longer than the
Welcome's `value_bytes_max` with `detail` that bound (`ValueTooLarge`, `STORAGE.md` section 3.3), at execution, before
the write is logged.

#### Scenario: A commit refused by delta admission says so
- **WHEN** a partition's delta stays at its watermark until a commit's deadline passes
- **THEN** the commit is refused with code 5, `detail` 1, and the client runs the transaction again on a new Begin

#### Scenario: A commit refused by grid admission says so
- **WHEN** a partition's grid holds fewer free extents than its reserve until `transaction_ms` from a one-frame write's arrival passes
- **THEN** the write is refused with code 5, `detail` 2, and the client sends the same request again after a delay

#### Scenario: A value past the bound is refused
- **WHEN** the Welcome names `value_bytes_max` 16,384, and a Write holds a string literal of 16,385 bytes
- **THEN** the Write is refused with code 6, `detail` 16,384, and nothing of it is logged
