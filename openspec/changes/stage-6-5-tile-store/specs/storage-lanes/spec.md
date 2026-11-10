## ADDED Requirements

### Requirement: Every read names one of three lanes
Every read request SHALL name a lane, L, S or X, and every family SHALL allow the three
(`specification/storage/STORAGE.md` section 18.1; `PROTOCOL.md` section 13.21). A Scan SHALL read in lane S, or in lane
X when it names a transaction; lane L, and a read of named rows, SHALL be a Frontier's (`PROTOCOL.md` section 13.13). A
Frontier's `lane` SHALL be 1 L, 2 S or 3 X, its `read_version` 0 for lane L and a version or version bound otherwise,
and its `transaction` not zero exactly for lane X (`PROTOCOL.md` section 13.21, Order).

#### Scenario: A lane X Frontier without a transaction is refused
- **WHEN** compute sends a Frontier of `lane` 3 whose `transaction` is all zero
- **THEN** the partition refuses it with `FieldValue` `"transaction"` before any frame

#### Scenario: A lane L Frontier names no version
- **WHEN** compute sends a Frontier of `lane` 1 with `read_version` 10
- **THEN** the partition refuses it with `FieldValue` `"read_version"`

### Requirement: The level and mode of a read decide its lane
Compute SHALL read by Frontiers in the lane and at the version the read takes (`PROTOCOL.md` section 13.12): a
read-write transaction's in lane X at the Bind's version; a read-only one at levels 0, 1 and 2 in lane S at the
`version` of the Entries of its first Scan of the partition, or at the version the client named; and a read the
classification makes monotonic-fast (`SCHEMA.md` section 9.6) at level 2 in lane L, `read_version` 0. Before any lane S
Frontier to a partition, compute SHALL fix the partition's version with a Scan of it, so a lane S Frontier never waits
for an epoch.

#### Scenario: A snapshot read's Frontier takes its Scan's version
- **WHEN** a level 0 read's first Scan of partition 2 answers Entries naming `version` `v`
- **THEN** every Frontier of the read to partition 2 is of lane 2 with `read_version` `v`

#### Scenario: A read-write transaction's Frontier is lane X
- **WHEN** a read-write transaction bound at version 200 reads a relation by Frontier
- **THEN** the Frontier is of lane 3, `read_version` 200, naming the transaction

### Requirement: A read merges tiles, the delta and its own writes
A read at version `v` of a key SHALL merge in key order the tile entries visible at `v`, the delta entries visible at
`v` and, in lane X, the transaction's own writes; lane L SHALL read the latest applied state with no version filter but
the deleted versions' (`STORAGE.md` section 18.2). A request whose tiles are not in the cache SHALL return `Miss` with
their references, its attempt undone, parked and rerun from its request once they are read (T33). In lane X a row SHALL
hold what the transaction added and lack what it removed, and a row it emptied SHALL be absent (`PROTOCOL.md` section
13.13).

#### Scenario: A read sees a change still in the delta
- **WHEN** a statement committed at 10 is in the delta and not yet in a tile, and a lane S read at 10 asks for its row
- **THEN** the read answers the statement

#### Scenario: A miss reruns the attempt
- **WHEN** a lane S Frontier's row lies in a tile not in the cache
- **THEN** the request is parked, the tile is read, and the rerun answers the row with no frame sent from the undone
  attempt

#### Scenario: Lane X sees its own writes
- **WHEN** a read-write transaction adds `<a> <knows> <c>` in a Write and then reads `<a>`'s `knows` row in lane X
- **THEN** the row holds `<c>` beside the entries committed at the read version

### Requirement: Lane L answers each turn's latest applied state
A lane L request SHALL read, in each frame, the replica's latest applied state at its turn, one frame per turn, each
Neighbours frame naming the version it read, and SHALL pin nothing and record no read set (`STORAGE.md` section 18.1;
`PROTOCOL.md` section 13.22). An entry inserted behind the resume key between frames SHALL not be answered. A lane L
request sent back to loading twice SHALL be answered in lane S at the replica's latest applied version at that moment,
its frames naming that version, and SHALL from then pin that version as lane S does, holding the fold horizon on the
primary (`STORAGE.md` section 18.6; `PROTOCOL.md` section 13.21).

#### Scenario: Lane L frames name their own versions
- **WHEN** a lane L Frontier takes two frames and a round is applied between them
- **THEN** the second frame names a higher `version` than the first

#### Scenario: Two misses move lane L to lane S
- **WHEN** a lane L Frontier misses the cache twice
- **THEN** its remaining frames name one version, the latest applied when it moved

### Requirement: Lane S holds one version at or above the logged horizon
A lane S request SHALL read one version `v` at or above the latest logged fold horizon for every frame, and each turn
SHALL first check that `v` is still at or above it, answering code 3 (TooOld) for the rest of the request otherwise
(`STORAGE.md` section 18.1). On the primary `v` SHALL hold the fold horizon until the request ends; on a follower it
SHALL hold nothing, and frames already sent SHALL be no answer (`PROTOCOL.md` section 11.9).

#### Scenario: A follower's lane S request falls behind the horizon
- **WHEN** a follower serves a lane S Frontier at 100 and applies a round logging `horizon` 150 before its next frame
- **THEN** the request is refused with code 3 for the rest of it

#### Scenario: The primary keeps the horizon for an open request
- **WHEN** the primary serves a lane S Scan at 100 that stays open
- **THEN** every round the primary logs meanwhile has `horizon` at most 100

### Requirement: The logged fold horizon is the only retention
A partition SHALL answer every version at or above its latest logged fold horizon and refuse a read below it with code 3
(TooOld) (`PROTOCOL.md` section 11.9; `STORAGE.md` section 18.6); no other setting or window SHALL bound which versions
a read may take. A Scan or Frontier of lane S or X whose `read_version` is below that horizon SHALL be refused with code
3, and one whose `read_version` is above the partition's visibility watermark in an epoch it has not closed with code 27
(AheadOfWatermark), `detail` the watermark (`PROTOCOL.md` sections 13.13 and 13.21). A resumed read naming a version
below the horizon SHALL answer code 3 as any read of it does (`PROTOCOL.md` section 11.8).

#### Scenario: A read below the horizon is too old
- **WHEN** a partition's latest logged `horizon` is 500 and a Scan with `at` 4 names `read_version` 400
- **THEN** the Scan is refused with code 3 before any frame

#### Scenario: A version above the watermark in an open epoch
- **WHEN** a Frontier of lane 2 names a version of the partition's open epoch above its visibility watermark
- **THEN** it is refused with code 27, `detail` the watermark

### Requirement: Resume keys are logical and name no slot
A request answering part of its result SHALL end with a resume key naming the last entry it answered by logical values
alone, laid out as `STORAGE.md` section 18.3 states: the graph's system ID; the entry's predicate's system ID for a
request naming every predicate; `esc` of the row's key, of the neighbour and of the ordering inline column's value; the
statement's system ID or 16 zero bytes; and, for a Frontier, the row's index in the request and the survivors it has
answered under `top`. The replica SHALL find the entry's family from the request, map its subjects and statement to slot
pairs through the slot maps and answer strictly after that position, whether or not the entry is still live. A key that
does not decode so, or names a family the request does not read, SHALL be refused with code 1 (Protocol), the message
naming the range.

#### Scenario: A resume key holds no slot
- **WHEN** a Scan's range of space 2 stops at its `limit` and its Entries frame gives a resume key
- **THEN** the key holds the graph's system ID, the row's and neighbour's logical bytes and the statement's system ID,
  and no slot pair

#### Scenario: A resume key past a deleted entry continues after it
- **WHEN** the entry a resume key names is deleted before the next Scan of the same range at the same version
- **THEN** the next Scan answers the entries strictly after the key's position

#### Scenario: A resume key of another family is refused
- **WHEN** a Scan range of kind 3 carries a resume key an earlier range of kind 1 gave
- **THEN** the Scan is refused with code 1, the message naming the range

### Requirement: Class I reads are stable and absences are always recorded
In lane X a class I entry or record a read finds SHALL never be recorded, in any read, and an absence SHALL always be
recorded (`STORAGE.md` section 18.4; `PROTOCOL.md` sections 13.13 and 13.21). A purge at version `v` SHALL abort every
open read-write transaction of its partition whose read version is below `v`. A relation whose liveness derives from its
participants' SHALL not be stable: compute SHALL keep its row only while each participant's `asserted_end` is absent,
read by a Frontier of kind 6 at the participant's home, which in lane X records the participant's forward row
(`PROTOCOL.md` section 11.12, *Reads*).

#### Scenario: A found class I value is not recorded
- **WHEN** a lane X Frontier of kind 6 finds a class I value for an instance's attribute
- **THEN** the transaction's reads record nothing for that row

#### Scenario: A purge aborts older read-write transactions
- **WHEN** a transaction reads at 100 and a purge commits at 120 in its partition
- **THEN** the transaction is aborted

#### Scenario: A participant's retraction conflicts with a relation read
- **WHEN** a read-write transaction keeps a relation row after a lane X kind 6 Frontier of its participant's
  `asserted_end`, and a retraction of the participant commits before the transaction's commit
- **THEN** the transaction's commit conflicts

### Requirement: What lane X records is the protocol's rule
Before a frame of a lane X Scan or Frontier leaves, the partition SHALL record in the transaction's reads what the frame
answers, as `PROTOCOL.md` sections 13.13 and 13.21 state: a range of space 1 of a claim, kinds 4, 5, 12 and 13, records
its claim's conflict key as a range holding that key alone, absent or found, a found class I claim aside; a range of
kind 18 records a found class M record's statement conflict key as a statement tested, or the absent statement's ID as
an absent term; a range of space 2 records its family's whole range of conflict keys when the partition takes it, the
graph's whole range for every predicate and the tenant's for every graph; a Frontier records the conflict-key range of
each row a frame reaches, entries or none, a frame ending inside a row recording its family's range in that row's bucket
group, a forward row's, or its whole family's otherwise, the graph's whole range for every predicate and the tenant's
whole range for every graph. What the read
set has no room for SHALL end the transaction with code 6 (TooLarge), `detail` the limit, sending no entry of that
range.

#### Scenario: A new row in a scanned family conflicts
- **WHEN** a lane X Scan of space 2 reads `knows` in graph `g` and another transaction then commits a new `knows` row in
  `g`
- **THEN** the first transaction's commit conflicts, however the range ended

#### Scenario: An absent claim conflicts with a later claim
- **WHEN** a lane X Scan of a key claim (kind 4) finds no record and another transaction then claims that key
- **THEN** the first transaction's commit conflicts

#### Scenario: An empty Frontier row is recorded
- **WHEN** a lane X Frontier reaches row `<a>` of `knows` holding no entry and another transaction then inserts `<a>
  <knows> <b>`
- **THEN** the first transaction's commit conflicts

### Requirement: A Scan reads records by key and families whole
A Scan SHALL read every range at one version (`PROTOCOL.md` section 13.13). A range of space 1 SHALL answer the record
of its kind and logical key in the tenant at that version, in whichever bucket group holds it, as one entry or none, and
a run of such ranges SHALL be answered in one frame; a range of space 1 of a replica's own kind, 6, 10, 11, 14 to 17 or
20, SHALL be refused by its `kind`. A range of space 2 SHALL read one predicate's family of its kind, or every
predicate's, in its graph or every graph, in every bucket group and both storage classes, the families in the order of
their family keys and each in its own order, answering each row holding an entry at the version as one entry, at most
`limit` rows; no order SHALL hold across bucket groups or partitions. A store failing under a range SHALL refuse the
Scan with code 17 (Unavailable), naming the range, so a failed read never reads as absence.

#### Scenario: A batch of terms is resolved in one frame
- **WHEN** a Scan holds three ranges of space 1 and kind 1, one per IRI
- **THEN** one Entries frame answers all three, `range` 0 and `through` 2

#### Scenario: A slot map is not readable
- **WHEN** a Scan holds a range of space 1 and kind 11
- **THEN** the Scan is refused by the range's `kind` before any frame

#### Scenario: A family read stops at its limit
- **WHEN** a range of space 2 with `limit` 2 reads a family holding three rows at the version
- **THEN** its last Entries frame has `end` 2 and a resume key, and a new Scan naming that key answers the third row

### Requirement: A Scan chooses its version once and waits within its deadline
A Scan SHALL choose its version once, when any wait for `epoch` has ended, by its `at` (`PROTOCOL.md` section 13.13): 1
at `read_version` once the partition has closed `epoch`; 2 at the latest applied version once it has; 3 at the version
bound of its latest closed epoch once that is at least `epoch`; 4 at a client-named `read_version` without waiting. A
`read_version` at or below the version bound of a closed epoch SHALL read what the partition applied at or below it. A
Scan whose `epoch` the partition has not closed within `deadline_ms` of its arrival SHALL be refused with code 24
(TimedOut), `detail` the partition, having sent no frame. Every Entries frame of a Scan SHALL name the one version it
chose.

#### Scenario: An idle partition answers a version it never reached
- **WHEN** a partition has closed epoch 3 and applied nothing in it, and a Scan with `at` 1 names epoch 3's version
  bound
- **THEN** the Scan answers what the partition applied at or below that bound

#### Scenario: An unclosed epoch past the deadline
- **WHEN** a Scan with `at` 1 and `deadline_ms` 0 names an epoch the partition has not closed
- **THEN** it is refused with code 24, `detail` the partition, at once

### Requirement: A stalled Scan or Frontier releases its version
A partition SHALL refuse a Scan or a Frontier that, once any wait is over, stays open with no frame able to leave, its
stream holding no credit or its connection taking no bytes, for `scan_stall_ms`, with code 24 (TimedOut), `detail` the
partition, the message saying it stalled, and SHALL release the version it held, so a stalled reader holds the fold
horizon for no longer than that; each frame that leaves SHALL restart the bound (`PROTOCOL.md` sections 11.9, 13.13 and
13.21).

#### Scenario: A reader that stops reading releases the horizon
- **WHEN** a lane S Frontier on the primary has a frame ready and its stream holds no credit for `scan_stall_ms`
- **THEN** the request is refused with code 24, the message saying it stalled, and the next round's `horizon` is no
  longer held at its version

### Requirement: A Frontier answers named rows in request order across groups and classes
A Frontier SHALL name its rows by logical key, never by slot, each at most once, a row named twice refused with
`FieldValue` `"row"` (`PROTOCOL.md` section 13.21). It SHALL answer the rows in the request's order and each row's
surviving entries in its family's order (`STORAGE.md` section 18.5). A row of kinds 1, 2 and 5 SHALL be read in its
subject's bucket group; a row of kind 6 in the column chunk family of the attribute and of the type its system ID's
prefix names, answering the slot's value as its one entry; a row of kinds 3 and 4 in every bucket group of the
partition, groups ascending, the families of its two storage classes merged within each group in the family's order. A
row of kind 5 SHALL answer its revisions, each its asserted start as a big-endian u64 and its value. A kind 3 key SHALL
be read in each family whose layout names rows in its form.

#### Scenario: Rows come back in the request's order
- **WHEN** a Frontier names rows `<c>`, `<a>`, `<b>` of `knows`
- **THEN** its Neighbours rows carry `row` 0, 1 and 2 in that order

#### Scenario: A reverse row spans bucket groups
- **WHEN** subjects of bucket groups 2 and 1 both hold `knows` edges to `<x>` and a Frontier of kind 3 names `<x>`
- **THEN** the row's entries of group 1 come before those of group 2

#### Scenario: A row named twice is refused
- **WHEN** a Frontier names the same key in rows 0 and 3
- **THEN** it is refused with `FieldValue` `"row"`, naming row 3

### Requirement: A Frontier may name every predicate of a kind
A Frontier whose `predicate` is 16 bytes of `0xFF` SHALL read a row in each predicate's family of the kind, in the order
of their family keys, each entry's neighbour bytes beginning with its predicate's system ID; such a request SHALL name
no filter and be answered with `widths` all zero and no inline values (`STORAGE.md` section 18.5; `PROTOCOL.md` sections
13.21 and 13.22).

#### Scenario: Every predicate's edges of a subject
- **WHEN** `<a>` holds `knows` and `likes` edges and a Frontier of kind 1 with every predicate names `<a>`
- **THEN** each entry's neighbour is 32 bytes, its predicate's system ID then the object's, and `widths` is all zero

#### Scenario: Every predicate with a filter is refused
- **WHEN** a Frontier naming every predicate holds `filters` 1
- **THEN** it is refused with `FieldValue` `"filters"`

### Requirement: A Frontier may name every graph
A Frontier whose `graph` is 16 bytes of `0xFF` SHALL read the row in the family of each graph the partition holds it
in, in the order of their family keys, each entry's neighbour bytes beginning with its graph's system ID, and, when its
`predicate` names every predicate too, the graph's system ID first and then the predicate's; in lane X it SHALL record
the tenant's whole range of conflict keys (`STORAGE.md` section 18.5; `PROTOCOL.md` sections 13.21 and 13.22). Its
resume key SHALL carry the graph as every resume key does (`STORAGE.md` section 18.3). Compute SHALL send it for a
pattern whose graph is unbound, as `PROTOCOL.md` section 13.12's table of patterns and kernels gives.

#### Scenario: A subject's edges in every graph
- **WHEN** `<a>` holds `knows` edges in the default graph and in `<g1>`, and a Frontier of kind 1 for `knows` with every
  graph names `<a>`
- **THEN** each entry's neighbour is 32 bytes, its graph's system ID then the object's, the default graph's entries
  first, its 16 zero bytes ordering before `<g1>`'s system ID

#### Scenario: A lane X read of every graph conflicts with a new graph
- **WHEN** a lane X Frontier with every graph reads `<a>`'s `knows` row, and another transaction then commits `<a>
  <knows> <b>` in a new graph `<g2>`
- **THEN** the first transaction's commit conflicts

### Requirement: Type ranges, inline filters and top-k select a row's survivors
An entry SHALL survive when its neighbour's type prefix lies in `[prefix_first, prefix_past)`, it passes every filter,
its column holding a value at or above `low` and at or below `high` compared as its sortable encoding compares, and it
is among the first `top` of its row that do; an entry with no value in a filtered column SHALL pass none, and a block
whose column's values cannot pass SHALL be skipped unread (`PROTOCOL.md` section 13.21; `STORAGE.md` section 18.5). Each
frame SHALL be one turn's work within `lane_entries_max` entries examined and `lane_bytes_max` bytes of body, and one
stopping with entries left SHALL end with its resume key and continue strictly after it in the next frame.

#### Scenario: Top-k stops at k
- **WHEN** a Frontier with `top` 2 names a row holding five surviving entries
- **THEN** the row answers its first two entries in its order, across frames if it is resumed

#### Scenario: A type range keeps one type's neighbours
- **WHEN** a kind 1 row holds neighbours of prefixes 3 and 4 and the Frontier names `prefix_first` 4, `prefix_past` 5
- **THEN** only the neighbours of prefix 4 survive

### Requirement: Results come partition by partition
Compute SHALL answer a read's partitions one after another in `PROTOCOL.md` section 9.1's order, into Result batches
with resume keys of its own that name the partition that answered the last row (`PROTOCOL.md` sections 11.8 and 11.12,
*Reads*). Resuming, compute SHALL pass over the partitions before that one, build the store's resume key from the last
row's terms in that partition, continue strictly after the row, its statement included, and read every later partition
from its start. A read that gave rows from a partition and then stalled SHALL continue in a new read at the same version
from the last frame's resume key; only one that stalled before giving a row SHALL refuse the read with code 24.

#### Scenario: A resumed read skips finished partitions
- **WHEN** a read over partitions 1, 2 and 3 is resumed with a key whose `partition` is 2
- **THEN** compute asks partition 1 nothing, continues partition 2 strictly after the key's row and reads partition 3
  from its start
