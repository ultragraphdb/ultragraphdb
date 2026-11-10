## ADDED Requirements

### Requirement: Ingest is a one-frame insert under DirectWrite's rules
An Ingest (command 26) SHALL open a stream of its own, need no Begin, and insert block-columnar statements and instances
and nothing else (`specification/binary_protocol/PROTOCOL.md` section 6.8; T25). Every rule of `PROTOCOL.md` section 6.6
SHALL hold of an Ingest and of each share except where section 6.8 states another: its request number from the session's
one counter, one commit in flight per session, a lost reply in doubt, the token, the checks of the session, the rights
and the tenant gate, and the blind predicates of a tenant that holds a schema. A client's frame SHALL carry `principal`
0 and `share` 0, each refused at the client endpoint with `FieldValue` naming the field.

#### Scenario: A client's share number is refused
- **WHEN** a client sends an Ingest with `share` 1
- **THEN** the frontend refuses it with `FieldValue` `"share"`, and no partition sees it

#### Scenario: A second commit in flight is refused
- **WHEN** a session holds an Ingest in flight and sends a DirectWrite
- **THEN** the DirectWrite is refused with code 14 (SessionBusy)

### Requirement: An Ingest frame is checked in its stated order
The receiver SHALL check an Ingest request in the order `PROTOCOL.md` section 6.8 gives: unused header bytes, `request`,
`token`, `principal`, `share`, the sections, section 4 empty, the term table, the blocks as whole 32-byte records and at
least one, then each block's fields and data part by part, refusing the first rule broken with the refusal that order
names. A block's rows SHALL not repeat a term an earlier row of the block names, a relation block's `ends` SHALL be
strictly increasing with the last `entries`, a column's `width` SHALL be at most the Welcome's `value_bytes_max`, and a
column's `present` SHALL equal its validity bits set. A reply SHALL be checked likewise: sections 0 to 3 empty, whole
48-byte outcomes, `share` numbered from 1, `partition` ascending, and a committed outcome's `block` and `row` `ABSENT`,
`detail` 0, `commit_version` a version and `position` at least 1.

#### Scenario: A repeated row is refused
- **WHEN** an entity block's `rows` part names one term index in rows 0 and 2
- **THEN** the frame is refused with `FieldValue` `"row"`, naming row 2

#### Scenario: A too wide column is refused
- **WHEN** a column declares `width` above the Welcome's `value_bytes_max`
- **THEN** the frame is refused with `FieldValue` `"width"`

#### Scenario: An unknown block kind is refused
- **WHEN** a block's `kind` is 3
- **THEN** the frame is refused with `FieldValue` `"block kind"`

### Requirement: The frontend refuses a frame whole before any relay
The frontend SHALL check the session and the rights as for a Commit (`PROTOCOL.md` section 11.5) and each term as
`PROTOCOL.md` section 4.4 states, refusing the frame whole. It SHALL refuse with code 2 (Conflict), kind 5 (Catalog),
before any partition sees it, a frame whose `catalog` is not the tenant's current version, the message naming both
versions (`PROTOCOL.md` section 6.8).

#### Scenario: A stale catalog version is refused at the frontend
- **WHEN** a tenant's current catalog version is 7 and a client sends an Ingest whose `catalog` is 6
- **THEN** the frontend refuses it with code 2, kind 5, the message naming 6 and 7, and no partition sees it

### Requirement: Each row goes to its subject's partition
The frontend SHALL find each row's partition as `PROTOCOL.md` section 11.12, *Writes*, finds a first request's
subject's: an IRI's bucket computed from its text and the tenant, or, for an IRI a keyed type mints, from the key space
and the key it states (`SCHEMA.md` sections 7.2 and 7.3); a skolem IRI's bucket as it states; a keyless instance's
bucket as its handle IRI states (`PROTOCOL.md` section 6.8).

#### Scenario: Rows of one block reach two partitions
- **WHEN** a relation block's rows name subjects `<a>` and `<b>` whose buckets two partitions own
- **THEN** the frame splits into two shares, each holding the block with its partition's row

#### Scenario: A keyed instance goes to its key's bucket
- **WHEN** an entity block of a keyed type holds a row naming the IRI its key mints
- **THEN** the row goes to the partition owning the bucket of the key space's prefix and the canonical key

### Requirement: New blank nodes form groups placed in one share
Every new blank node of a frame SHALL be created in exactly one share (T43): the frame's new blank nodes, joined
wherever a row whose subject is one names another as a neighbour, SHALL form groups, and a group SHALL go to the
partition of the buckets the `@idempotency` values of its keyless entities derive (`SCHEMA.md` section 7.1), of the
anchor of each node-encoded relation instance it holds, and of the rows whose subjects are IRIs or skolem IRIs and that
name one of its nodes as a neighbour (`PROTOCOL.md` section 6.8; T48, C47). A row whose subject is a new blank node
SHALL go with its group. A group these place in two partitions SHALL refuse the frame with code 23 (CrossPartition)
before any relay, the message naming the blank node and the first two partitions in frame order, block by block and row
by row.

#### Scenario: An idempotency value places its group
- **WHEN** a keyless entity's row under `_:n` holds an `@idempotency` value and another row names `_:n` as a neighbour
  of `_:m`
- **THEN** both rows go to the partition of the bucket the value derives

#### Scenario: A relation instance follows its anchor
- **WHEN** a new node-encoded relation instance `_:r` names `<a>` in its anchor role
- **THEN** `_:r`'s rows go to the partition owning `<a>`'s bucket

#### Scenario: A group placed twice is refused
- **WHEN** `<a>` and `<b>` of two partitions both name `_:n` as a neighbour
- **THEN** the frame is refused with code 23, the message naming `_:n` and both partitions, before any relay

### Requirement: An unplaced group follows the session or a round-robin choice
A group no rule places SHALL go, in a frame through a session, to the partition at position XXH3-64, seed 0, of the
frame's `session` and `request` command-header bytes, modulo the count, among the partitions holding the tenant's run in
ascending order (`PROTOCOL.md` section 6.6); in a frame through no session, every such group of the frame SHALL go to
one partition the frontend chooses in turn for the frame, round-robin over the partitions holding the tenant's run
(`PROTOCOL.md` section 6.8). The placement of a frame through a session SHALL read the frame and the frontend's map
alone, so a resend splits alike.

#### Scenario: A resend places its blank nodes alike
- **WHEN** a frame through a session whose blank-node groups no rule places is sent and then resent with the same
  `session` and `request`
- **THEN** both go to the same partition

#### Scenario: A sessionless frame keeps its groups together
- **WHEN** a frame through no session holds two unplaced blank-node groups
- **THEN** both go to one partition, and the next such frame goes to the next partition in turn

### Requirement: Shares are relayed with the frame's header and term table
The frontend SHALL relay a share to each partition reached, on a new stream to its role address, holding the frame's
command header with `principal` and `share` filled, its term table whole, and of each block the rows that partition owns
with their neighbours and their columns' values, in their order; a block with no such row SHALL be left out, and the
share's blocks and rows SHALL be numbered anew (`PROTOCOL.md` section 6.8). Shares SHALL be numbered from 1 in ascending
order of their partitions.

#### Scenario: A share leaves out a block it holds no row of
- **WHEN** block 0's rows all reach partition 1 and block 1's rows reach partitions 1 and 2
- **THEN** share 2, to partition 2, holds one block, numbered 0, with block 1's partition 2 rows

### Requirement: Each share commits atomically and on its own
A partition SHALL execute its share in one turn at its visibility watermark and commit it through the commit step as a
transaction of its own; a refused share SHALL write nothing of any block, while the other shares commit or fail on their
own (`PROTOCOL.md` section 6.8; T23, T24). Its outcome SHALL be 0 with the commit version and the round's position,
which with `partition` and the commit version's epoch is the share's session token, or the refusal's code with its first
failing block and row and `detail`.

#### Scenario: One refused share leaves the others committed
- **WHEN** an Ingest splits into shares 1 and 2 and share 2's partition refuses a row's value
- **THEN** share 1 commits, share 2 writes nothing of any block, and the reply holds outcome 0 for share 1 and code 7
  for share 2

### Requirement: A partition refuses a share it cannot take
A partition SHALL refuse its share in the share's outcome (`PROTOCOL.md` section 6.8): with code 2, kind 5, when
`catalog` is not the version in effect at the round's epoch; with code 7 (Refused) and the block named for a block
naming a type, an attribute or a predicate the version does not declare, a column whose `width` is not its descriptor's
sortable width, a block whose type, predicate or column's predicate is not blind to write but for its type's key and
`@idempotency` claims (`SCHEMA.md` section 9.6), the message naming the predicate and a read-write transaction, or a
column or an entity block in a tenant holding no schema; with code 7 and the row named for a value its check refuses;
and with code 6 (TooLarge), `detail` the bound, for a share whose body and resolution do not fit one round of its
partition's log beside that round's header and tables.

#### Scenario: An entity block in a tenant with no schema is refused
- **WHEN** a share holding an entity block reaches a partition of a tenant holding no schema
- **THEN** the share is refused with code 7, `block` the entity block

#### Scenario: A predicate needing a read-write transaction is refused
- **WHEN** a governed tenant's share holds a column of a `@unique` attribute
- **THEN** the share is refused with code 7, the block named, the message naming the predicate and a read-write
  transaction

#### Scenario: A share too large for a round is refused
- **WHEN** a share's body and resolution do not fit one round of its partition's log
- **THEN** the share is refused with code 6, `detail` the bound

### Requirement: A share claims its instances' keys
A share SHALL check the key claims of the instances it creates in its turn, since a key's claim lives in the instance's
own bucket (`SCHEMA.md` sections 7.2 and 9.3; T42). A row whose key an instance of its key space already claims, or that
a row of the same share claims before it, SHALL refuse its share with code 7 (Refused), the row named and `detail` 1008.

#### Scenario: A key claimed by an existing instance refuses the share
- **WHEN** a share's entity block holds a row whose key an existing instance of its key space claims
- **THEN** the share is refused with code 7, `detail` 1008, naming that block and row, and writes nothing

#### Scenario: A key repeated within a share refuses the share
- **WHEN** rows 0 and 3 of one share's entity block hold the same key
- **THEN** the share is refused with code 7, `detail` 1008, naming row 3

### Requirement: An idempotency value already claimed resolves to its instance
A row whose `@idempotency` value an instance of its type already claims SHALL write none of its values, and the share's
rows naming its blank node SHALL name that instance; a row whose value an earlier row of the same share claims SHALL do
the same, its blank node naming the first row's instance (`PROTOCOL.md` section 6.8; `SCHEMA.md` section 6.6; T48, C45).
Every other claim, and every rule reading beyond the instance, SHALL stay a read-write transaction's.

#### Scenario: A retried Ingest finds its instance
- **WHEN** a share's row under `_:o` holds an `@idempotency` value an existing instance `i` of its type claims, and
  another row names `_:o` as a neighbour
- **THEN** the share commits, row `_:o` writes no value, and the other row's edge names `i`

#### Scenario: A duplicate value within a share names the first row's instance
- **WHEN** rows 0 and 1 of a share's entity block, under `_:a` and `_:b`, hold the same `@idempotency` value
- **THEN** row 0 creates the instance, row 1 writes no value, and rows naming `_:b` name row 0's instance

### Requirement: A share's round logs its body as received with its resolution
A committed share's round SHALL log its body as its frame carried it in the Round's section 4, block-aligned, and its
resolution in section 5, and no write-set record (`PROTOCOL.md` section 12.1). The resolution SHALL hold the tenant, the
catalog version in effect at the round's epoch, the `read_version` it executed at, the share's `session`, `request`,
`principal` and `share`; per entry of the body's term table the system ID it resolved to, 16 zero bytes for a literal
and an unnamed term; the statement IDs it gave edges, by block and entry; and the skipped list, each row it skipped
whole for its `@idempotency` claim and each edge it skipped because the edge was visible at its `read_version`,
ascending by block, row and entry. The conflict keys of the share's inserts SHALL not be logged.

#### Scenario: A skipped row is listed
- **WHEN** a share skips row 1 of block 0 for its `@idempotency` claim
- **THEN** the resolution's section 2 holds the entry `block` 0, `row` 1, `entry` `ABSENT`

#### Scenario: An edge already visible is listed
- **WHEN** a share's relation block holds, as neighbour 4 of row 2 of block 1, an edge visible at its `read_version`
- **THEN** the resolution's section 2 holds the entry `block` 1, `row` 2, `entry` 4, and apply adds no entry for it

#### Scenario: A resolution entry inside a skipped row is refused
- **WHEN** a Round's resolution lists row 1 of block 0 skipped whole and also an edge of that row
- **THEN** the Round is refused with `FieldValue` `"resolution skipped"`

### Requirement: Apply and replay take a share from its body and resolution
Apply and replay SHALL derive a share's statements from its body and resolution, as quads of the resolved system IDs and
the column values with the statement IDs the resolution gives, skip each row and edge the skipped list names, and add
the rest, giving slots in the order `STORAGE.md` section 14.6 states for an Ingest body (`PROTOCOL.md` sections 12.1 and
12.2). The share's session-table entry SHALL be its resolution's `session`, `request` and `share` with the share's
Ingest reply, rebuilt from the round.

#### Scenario: Replay rebuilds a share
- **WHEN** a partition replays a round holding a committed Ingest share whose resolution skips one row
- **THEN** its state holds every statement of the share but that row's, and its session table holds the share's reply

### Requirement: The frontend answers once with every share's outcome
Once every share is answered, the frontend SHALL answer the client with one Ingest reply holding the request's command
header as the frontend took it, sections 0 to 3 empty, and every share's outcome by `share`, naming each refused share's
block and row by their indices in the client's frame (`PROTOCOL.md` section 6.8). A partition's reply SHALL hold its
share's one outcome.

#### Scenario: A refused row is named in the client's numbering
- **WHEN** share 2's partition refuses the share's block 0, row 0, which is the client frame's block 1, row 3
- **THEN** the client's reply names `block` 1 and `row` 3 in share 2's outcome

### Requirement: A resend commits only the shares not yet committed
A resend of an Ingest SHALL be split and relayed as the first send was, and each partition's session table, keyed by the
session alone and holding the share's number beside the request (`PROTOCOL.md` section 11.5; `STORAGE.md` section 9.3,
kind 10; T48, C3), SHALL decide its share as `PROTOCOL.md` section 6.6 states: the stored reply for its last request,
code 14 for a request in its commit step, code 13 below its last, and otherwise committing it. A resend reaching a
partition under another share number after the placement map changed SHALL find its entry. A resend SHALL commit only
the shares not yet committed and answer the rest from the tables.

#### Scenario: A resend after a partial outcome
- **WHEN** an Ingest's share 1 committed, share 2's partition stopped before deciding it, and the client resends the
  Ingest with the same session and request
- **THEN** share 1 is answered with its stored reply and commits nothing again, and share 2 commits

#### Scenario: A share number changed by the map finds its entry
- **WHEN** a resend reaches a partition that committed the original as share 2 and now numbers it share 1
- **THEN** the partition answers the stored reply and commits nothing again
