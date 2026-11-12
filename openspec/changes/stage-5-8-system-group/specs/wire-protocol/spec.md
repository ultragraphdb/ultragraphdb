## ADDED Requirements

### Requirement: The system group's commands are bodies of the protocol
The protocol SHALL carry, as role commands with the bodies `PROTOCOL.md` sections 13.8, 13.9, 13.23 to 13.25, 13.27
and 13.28 state: Challenge (519) and Verify (520), served from stage 5.8; Session (534), a session's identity and its
principal, answered with the same bytes; Check (535), one section of 32-byte checks, each a session, a principal, a
`kind` of 1 or 2 and an `outcome`, answered whole with every outcome filled; Ended (536), one section of sessions,
answered by nothing, its stream ending with it; Declare (538), a `kind` of 1 or 2 with the tenant, the version built on and the principal, and
the sections of a Write; and Defaults (539), a tenant's default-entry predicates as a term table, continued across
frames as Catalog is. Every decoder SHALL enforce its body's order of checks, and a builder SHALL refuse to build what
its decoder refuses. The command table SHALL keep number 540 for stage 6.6 and refuse it as an unknown command.

#### Scenario: A Check's sessions ascend
- **WHEN** a Check holds two checks whose sessions are equal, or the second below the first
- **THEN** it is refused with `FieldValue` naming `check` and the second check's index

#### Scenario: A Check request holds no outcome
- **WHEN** a Check request holds an outcome other than 0, or a reply an outcome of 0 or 7
- **THEN** each is refused with `FieldValue` naming `check` and the check's index

#### Scenario: A Subscribe from the latest close names no epoch
- **WHEN** a Subscribe holds `latest` 1 with `from` 5, or `latest` 1 on a connection whose role is `storage`
- **THEN** each is refused with `FieldValue` naming `latest`

#### Scenario: A Catalog request may name an epoch only without a version
- **WHEN** a Catalog request names `version` 3 and `epoch` 9
- **THEN** it is refused with `FieldValue` naming `catalog`

#### Scenario: An Ended names at least one session
- **WHEN** an Ended holds an empty section 0, or a section of 24 bytes
- **THEN** the first is refused with `CountMismatch` naming `sessions`, the second with `SectionLength`

#### Scenario: A Session names a session
- **WHEN** a Session's `session` is all zero
- **THEN** it is refused with `FieldValue` naming `session`

#### Scenario: A Declare of a Drop holds no declaration
- **WHEN** a Declare of kind 2 holds a record in section 0, or names `drop` 1 with no term
- **THEN** the first is refused with `CountMismatch` naming `declarations`, the second with `CountMismatch` naming
  `drop terms`

#### Scenario: Defaults continues across frames in order
- **WHEN** a tenant's default-entry predicates fill two Defaults frames
- **THEN** the first carries no `LAST` and the second does, both hold one command header, and the predicates ascend by
  their texts' bytes across both

#### Scenario: Each new body has a golden frame
- **WHEN** the golden frames of Session, Check, Ended, Declare, Defaults, Challenge and Verify are decoded and built
  again
- **THEN** each decodes to the fields its generator names and builds to the same bytes

#### Scenario: Command 540 is not served
- **WHEN** a role connection sends command 540
- **THEN** it is refused as an unknown command

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
only, and rows SHALL come in the store's key order. The frontend SHALL answer Describe itself, from the catalog version
in effect, read from the system group with Catalog, and the tenant's default-entry records, read with Defaults, asking
no partition, as far as the connection's principal may read it, so it names what is declared and recorded rather than
what data holds: `read_version` the version bound of the latest epoch the sequencer closed, or of epoch 0 before the
first close; the entity types declared in effect at that epoch; the predicates of kind edge or either that it declares
or holds a default entry for as relationship types, those of kind property or either as property keys; and the catalog's
declarations. An answer not fitting one frame within the client's `max_frame` SHALL be refused with code 6 (TooLarge),
`detail` that `max_frame`.

#### Scenario: A stale statement is refused
- **WHEN** a statement is prepared, the catalog of its tenant changes, and the statement is executed
- **THEN** the execution is refused as stale, and preparing it again succeeds

#### Scenario: Describe answers from the metadata group
- **WHEN** `name` is declared a property with a `term` index, `score` holds a default entry, and a client sends Describe
  for the tenant
- **THEN** the frontend answers asking no partition: `read_version` is the version bound of the latest closed epoch,
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
Scan's range SHALL carry `after_len` and its `after`; ComputeRead SHALL carry `max_frame` and `credit`; a Reference's
`outcome` SHALL be 0, 7, 17 or 26. A dialling side SHALL refuse an Entries frame whose `version` is not its Scan's first
frame's, with `FieldValue` naming `version`, and an entry that does not belong where its frame puts it, with
`FieldValue` naming `entry order`, the entry's index: in a run, one whose key is not the first key of a range from
`range` through `through`, at or after the range of the entry before; in a batch of one range, a key below the range's
first key, or at or above its past key when it has one.

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
