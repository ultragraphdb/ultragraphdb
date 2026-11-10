## ADDED Requirements

### Requirement: Families follow the layout of the round's catalog version
The engine SHALL instantiate adjacency and column chunk families from the layout of the tenant's catalog version that
the round's records name, read through the `Layout` interface: for a tenant with no schema the built-in generic layout,
which holds for every (graph, predicate) a forward family of kind 1 for entity objects, of kind 2 for other terms, a
reverse family of kind 3, the index families its tokenizers declare, and no inline column; for a governed tenant the
catalog version's Layout entry, decoded and never recompiled (`specification/storage/STORAGE.md` section 13.4). Every
tenant, whatever its layout, SHALL keep the record families `STORAGE.md` section 9.2 scopes, and for each (bucket group,
type) holding a slot an identity column and a slot map (T45). A family SHALL be instantiated per graph and bucket group
when its first change there arrives, and its tiles SHALL be ordered by tile key (`STORAGE.md` section 13.1).

#### Scenario: A generic tenant's statements reach a forward and a reverse family
- **WHEN** the engine applies the rounds of `specification/conformance/storage/vectors/engine.json` under its settings
- **THEN** every tile it writes lies in the family whose family key is `families.forward.family_key` or
  `families.reverse.family_key`, of kinds 1 and 3

#### Scenario: A literal object takes the forward family of kind 2
- **WHEN** a tenant with no schema commits `<a> <name> "Ann"` and `<a> <knows> <b>`
- **THEN** the `name` statement is a forward entry of kind 2 and the `knows` statement a forward entry of kind 1, each
  with a reverse entry of kind 3

#### Scenario: A tenant with no schema keeps slot maps and identity columns
- **WHEN** a tenant with no schema commits its first statement with subject `<a>`
- **THEN** `<a>`'s slot is held in a slot map (record kind 11) and an identity column (column kind 3) of its bucket
  group and type

### Requirement: Slots are given per bucket group and type in log order
A replica SHALL give a slot to an entity of its own buckets per (bucket group of its bucket, type of its prefix), the
next after the highest given, from 0, the first time a committed record names it as a subject or an instance: in round
order, then record order, then the order of the record's sections 0 to 2 and their entries; in an Ingest body, its
blocks in order, each block's rows in order, a row's subject before its neighbours, then the resolution's statement IDs,
passing over the rows and entries the resolution's skipped section lists (`STORAGE.md` section 14.6; `PROTOCOL.md`
section 12.1). A row key, a neighbour and a statement SHALL name a slot as the pair (type prefix, slot); a column chunk
SHALL name the slot alone. Slots SHALL never be reused; an entity of another partition SHALL take no slot; and no round,
WAL record, answer or resume key SHALL name a slot.

#### Scenario: Two types each start at slot 0
- **WHEN** a round's first record names a subject of prefix 3 and then a subject of prefix 4, both of one bucket group
- **THEN** both take slot 0, named as the pairs (3, 0) and (4, 0)

#### Scenario: A deleted entity's slot stays a hole
- **WHEN** an entity holding slot 5, the highest given, is deleted and a new entity of its bucket group and type is
  created
- **THEN** the new entity takes slot 6 and slot 5 is not given again

#### Scenario: A skipped Ingest row takes no slot
- **WHEN** an Ingest share's resolution lists row 1 of block 0 skipped whole and rows 0 and 2 name new subjects of one
  type
- **THEN** row 0's subject takes the next slot and row 2's the one after it

### Requirement: The delta holds committed changes in a fixed pool
The delta SHALL hold every committed change not yet in a tile, as delta rows of one row of one family and overlay
changes to column chunks and records, each change an entry of its tile shape with an operation and its transaction's
commit version, every delta row in commit order and so sorted by version (`STORAGE.md` section 14.1). Its entries SHALL
be counted from a pool of `delta_entries_max` entries allocated at startup, every change counting one, and the delta
SHALL never exceed the pool (`PoolBound`, `STORAGE.md` section 19.3).

#### Scenario: Each statement is a forward and a reverse change
- **WHEN** the engine applies round 1 of `engine.json`, eleven inserted statements
- **THEN** the delta holds 22 entries, as `admission[0].delta` records

#### Scenario: A delta row is sorted by version
- **WHEN** commits at versions 10, 12 and 14 change one row
- **THEN** the row's delta changes are in the order 10, 12, 14

### Requirement: Visibility is decided by created and deleted versions and purge markers
At version `v` an entry SHALL be visible when its created version is at or below `v` and its deleted version is 0 or
above `v`; an entry of a tile that is not `versioned` SHALL be live at every version a read may take. A delta delete
SHALL name one entry by row key, neighbour and statement (a column record by its slot, a record by its key) and set the
deleted version of the one entry with those values visible at the version before it; a delete of a statement without a
statement ID SHALL delete every visible entry of its row key and neighbour. An entry naming an instance a purge marker
(kind 20) names SHALL be absent at every version, whatever its class, the marker, the slot map and the identity column
aside (`STORAGE.md` section 14.4).

#### Scenario: A delete hides an entry from later versions only
- **WHEN** an entry created at 10 is deleted at 20
- **THEN** a read at 15 finds it and a read at 20 does not

#### Scenario: A delete without a statement ID removes every statement slot
- **WHEN** a row and neighbour hold entries under statements `t1` and `t2` and a delete with an empty statement ID names
  the row and neighbour at 30
- **THEN** a read at 30 finds neither entry

#### Scenario: A purged instance is absent below the purge
- **WHEN** an instance's entries were created at 10 and a purge marker naming it is written at 40
- **THEN** a read at 20 finds no entry whose row key, neighbour or statement names the instance

### Requirement: The fold horizon is logged by the primary and folded at by every replica
The primary SHALL stamp every round with the fold horizon `H`: the least of its oldest open read-write transaction's
read version, the oldest version a lane S read holds, the start version of every index build in progress as its build
record holds it, and `R`, the version bound of the latest epoch whose logged close time is at or before the close of the
round's epoch less one minus `retention_ms`; and never below the round before's (`STORAGE.md` section 14.2). Every
replica and replay SHALL fold at the logged value, never a local one, and the logged `H` SHALL never fall
(`HorizonMonotone`). Replay SHALL refuse a round whose `horizon` is below the round before's, naming both horizons, and
apply nothing of it (`PROTOCOL.md` section 12.1).

#### Scenario: A build in progress bounds the horizon
- **WHEN** an index build started at version 100 is in progress and `R` is 200, with no older transaction or snapshot
- **THEN** the round's logged `horizon` is at most 100

#### Scenario: The horizon never falls
- **WHEN** the oldest open transaction's read version would give a horizon below the previous round's
- **THEN** the round logs the previous round's horizon

#### Scenario: Replay refuses a falling horizon
- **WHEN** a replayed round's `horizon` is below the round before's
- **THEN** replay refuses it naming both horizons and applies nothing of it

### Requirement: Storage classes decide which versions a tile keeps
A family SHALL take its storage class from the layout: class I holds instances of `@append_only` types with their
attributes and keys, `@bitemporal` content as version runs, and the record kinds `STORAGE.md` section 9.2 makes class I,
and keeps only the created version while above `H`; class M holds everything else, the term-to-ID dictionary among it,
and keeps created and deleted versions while either is above `H` (`STORAGE.md` section 14.3). A class I entry SHALL be
removed only by a purge. A predicate held by types of both classes SHALL be two families of every shape, one per class.

#### Scenario: A class I tile holds no deleted versions
- **WHEN** compaction writes a tile of an `@append_only` type's family holding an entry created above `H`
- **THEN** the tile's header carries `class_i` and its `versioned` sections hold created versions and no deleted ones

#### Scenario: A predicate of both classes splits
- **WHEN** a governed tenant's `name` attribute is held by an `@append_only` type and by a deletable type
- **THEN** its forward entries lie in two families whose tile keys differ in the class byte

### Requirement: Folding removes what the horizon passed
When compaction writes an entry it SHALL remove an entry deleted at or below `H`, give an entry created at or below `H`
created version 0, fold the records of one key of an additive kind (7, 9 and 19) created at or below `H` into one record
created at 0 whose value is their fold, and remove every entry a purge marker makes absent, class I included
(`STORAGE.md` section 14.5). A tile SHALL be `versioned` exactly when one of its entries has a created version above `H`
or, in class M, a nonzero deleted version.

#### Scenario: A column chunk's overlay folds with conservation
- **WHEN** the input chunk of `specification/conformance/storage/vectors/compaction.json`'s `column_fold` is rewritten
  at its `horizon`
- **THEN** the output encodes to its `output_file`, its added and removed entries are `added_hex` and `removed_hex`, and
  the input and output digests conserve

#### Scenario: Additive statistics records fold into one
- **WHEN** the run of `kinds.json`'s `record_statistics_additive` is folded at a horizon above every record
- **THEN** the key holds one record created at 0 whose value is the vector's `fold_hex`

### Requirement: Apply reads the round and adds
Apply SHALL turn a committed round into delta changes reading only the round, the catalog versions it names, the delta
and the slot maps, and no other tile (`STORAGE.md` section 15.1): every derivation that needs a tile runs during
execution, and the round carries its result in its records or its derivations section (`PROTOCOL.md` section 12.1,
section 6). Apply SHALL write statistics (kind 9) and `@limit` aggregates (kind 19) from derivation kinds 3 and 2 as
additive records, reading no prior value, and SHALL derive referrer-set members (kind 7) from the records'
registrations. Execution SHALL pin the slot-map pieces a round names from its execution until the round applies, and
recovery SHALL load them before it applies a replayed round.

#### Scenario: A count row moves by the logged degree
- **WHEN** a round adds two `friend` edges from `<a>` and its derivations section holds derivation kind 1 for `<a>`'s
  subject count with degree 3
- **THEN** apply deletes the count index entry of degree 3 and inserts the entry of degree 5, reading no tile

#### Scenario: Apply reads no data tile
- **WHEN** a round is applied whose slot-map pieces are pinned in the cache
- **THEN** apply issues no tile read

### Requirement: A statement's changes follow the layout
For a statement `(g, s, p, o)` committed at version `v` in the subject's partition, apply SHALL insert a forward entry
in `(g, p, forward)`, of kind 1 when `o` is an entity and kind 2 otherwise, keyed by `s`'s slot pair; a reverse entry in
`(g, p, reverse)` of kind 3 with `s`'s slot pair as its neighbour, none for a vector literal, which no reverse key
names; and an entry per token in each index family the
layout declares for `p`; each carrying the statement's slot pair or `ABSENT`, the inline values the layout places and
`v` (`STORAGE.md` section 15.2). A compiled single-valued attribute SHALL be a column overlay record, a `@bitemporal`
revision SHALL append to its version run, and a deleted statement SHALL be the same changes as deletes.

#### Scenario: A deleted statement is deleted in every family
- **WHEN** a statement with an index family of `p` is inserted at 10 and deleted at 20
- **THEN** reads at 20 find neither its forward, its reverse nor its index entries, and reads at 10 find all three

### Requirement: Records, sessions and evictions apply at their versions
Apply SHALL write dictionary entries, claims, marks, referrer members, tombstones, statement records, session entries,
purge markers, drop markers and build records as record changes of their kinds at the entry's version, and a slot given
as a slot map record and an identity overlay record (`STORAGE.md` section 15.3). A term re-created after its delete
SHALL delete its old term-to-ID record at the re-creation's version, its new ID-to-term record naming the ID it
replaced; an instance's delete SHALL delete its idempotency claim. A round's evictions SHALL delete each named session's
record at the round's `first_version` before the round's entries apply, a round with no committed entry consuming the
epoch's next unused version for nothing else.

#### Scenario: A read below a re-creation resolves the old ID
- **WHEN** IRI `<x>` held ID `i1`, its entity was deleted, and `<x>` is created again at 50 with ID `i2`
- **THEN** a read of `<x>` at 49 resolves `i1`, one at 50 resolves `i2`, and `i2`'s ID-to-term record names `i1`

#### Scenario: An eviction takes the round's first version
- **WHEN** a round holding no committed entry evicts session `S`
- **THEN** `S`'s session record is deleted at the round's `first_version`, and no commit takes that version

### Requirement: Statistics and aggregates are written once per round
At the end of each round apply SHALL write one statistics record per (graph, predicate) the round changed and one
`@limit` aggregate record per aggregate the round changed, created at the round's last commit version, holding the
change derivation kinds 3 and 2 carry (`STORAGE.md` section 15.4). A predicate's statistics at a version SHALL be the
merge of its records visible there (`STORAGE.md` section 9.4).

#### Scenario: Two rounds' statistics merge
- **WHEN** one round adds two `knows` statements and the next removes one, before any rewrite
- **THEN** two statistics records of `knows` exist, and a read after both answers a statement count of 1

### Requirement: Admission counts each round's own changes
The primary SHALL admit an entry to a round only while the delta's entries, with the changes of the round's entries
before it and its own, stay at or below `delta_watermark_percent` of `delta_entries_max`, and `free` stays at or above
the reserve with those changes counted (`STORAGE.md` sections 12.4 and 15.6). An entry that does not join SHALL wait for
compaction or a checkpoint, a transaction's commit until its transaction's deadline and a one-frame write, a Drop or an
Ingest share until `transaction_ms` from its arrival, as `PROTOCOL.md` section 11.12 counts a one-frame write in a
queue (`STORAGE.md` section 15.6); past that it SHALL be refused with code 5 (Busy), retryable, `detail`
1 for delta admission and 2 for grid admission (`GridFull`) (`PROTOCOL.md` section 9.4). While `GridFull` holds and the
delta holds changes or an extent has been released since the last checkpoint, the primary SHALL log a round flagged
`checkpoint`; with neither, `GridFull` SHALL persist and refuse every commit with code 5, `detail` 2.

#### Scenario: The reference run stays within admission
- **WHEN** the engine applies the rounds of `engine.json`
- **THEN** at each round the delta with the round's changes counted, the free extents and the reserve equal the round's
  `admission` record, every delta at or below 75% of `delta_entries_max` and every `free` at or above its `reserve`

#### Scenario: A full delta refuses with detail 1
- **WHEN** the delta stands at its watermark and a commit's changes would pass it until the commit's deadline
- **THEN** the commit is refused with code 5, `detail` 1, and writes nothing

#### Scenario: A grid holding only live data refuses for good
- **WHEN** `free` is below the reserve, the delta is empty and no extent was released since the last checkpoint
- **THEN** every commit is refused with code 5, `detail` 2, and no round is flagged `checkpoint`

### Requirement: Due tiles are queued by reason, size and key
At every round boundary, after apply, the engine SHALL find each tile due by the five reasons of `STORAGE.md` section
16.1, a tile's reason the first it meets in the order 1, 4, 5, 2, 3, and SHALL queue due tiles by reason in that order,
then by `d` descending, then by tile key ascending, evaluated at the boundary they become due; a tile already queued or
taken by a rewrite in progress SHALL not be queued again (`STORAGE.md` section 16.2).

#### Scenario: The queue's order
- **WHEN** the tiles of `specification/conformance/storage/vectors/decisions.json`'s `queue` become due
- **THEN** they are queued in the vector's `order`, `b`, `f`, `e`, `c`, `a`, `d`

#### Scenario: A row past its bound is due first
- **WHEN** one delta row of a tile holds `delta_row_entries_max` changes and the tile also meets reason 2
- **THEN** the tile's reason is 1

### Requirement: A rewrite's inputs are fixed at its deciding boundary
A rewrite SHALL start at the boundary its tile leaves the queue's head, its deciding boundary, and take as inputs the
tile, the delta changes belonging to it there and the `H` logged at that boundary's round; changes applied later SHALL
stay in the delta (`STORAGE.md` section 16.3). It SHALL also take the family's next tile when the first holds fewer than
`tile_entries_max / 4` entries, the two hold at most `tile_entries_max` together and the next is not an input of a
rewrite in progress; a family's last tile and a column chunk take none. A family with no tile SHALL have one rewrite of
its new tile, from its delta changes alone.

#### Scenario: Rewrites decided before they complete
- **WHEN** the engine applies the rounds of `engine.json`
- **THEN** every rewrite completes at the boundary and with the deciding boundary its `events` record, the ten whose
  `position` differs from `decided` included, with the inputs and outputs the event names

#### Scenario: A small tile merges with its neighbour
- **WHEN** a due tile holds fewer than `tile_entries_max / 4` entries and the family's next tile fits beside it
- **THEN** one rewrite takes both, and the next tile leaves the queue

### Requirement: A rewrite merges, folds, cuts, checks and stamps
A rewrite SHALL merge its inputs and delta changes in their shape's order, applying each delete to the entry it names;
fold at `H`; cut the result greedily into outputs of at most `tile_entries_max` entries by whole rows, a larger row cut
into pieces of exactly `tile_entries_max`; halve an output longer than `tile_bytes_max` at the row boundary nearest its
middle, the lower on a tie; check conservation (`DigestConserved`) and decode each output once (`RoundTrip`); and stamp
`built` with the deciding boundary's position, `previous` with the first input's `built` or 0 for a new tile, and
`horizon` with `H` (`STORAGE.md` sections 11.2 and 16.3). A rewrite with no output SHALL remove its inputs and place
nothing. A rewrite SHALL place its outputs in the order tile-grid's "Placement order within a boundary" gives; the
outputs SHALL replace the inputs in the directory at the boundary the rewrite completes, and the delta changes it merged
SHALL be freed from the pool.

#### Scenario: Cuts follow rows
- **WHEN** rows of `decisions.json`'s `cuts` are cut at their `tile_entries_max`
- **THEN** the outputs are each case's `outputs`, a hub row cut into full pieces with its last piece open

#### Scenario: A hub row is halved until it fits
- **WHEN** the 4,096 entries of `compaction.json`'s `halving` are encoded under its settings
- **THEN** the output is cut into the vector's `pieces`

#### Scenario: Outputs land at the reference addresses
- **WHEN** the engine applies the rounds of `engine.json`
- **THEN** every output lands at the address, with the length and checksum, its event names, and the grid after round
  220 equals `tiles/engine_grid.blocks`

### Requirement: Compaction is paced by units the rounds earn
Each applied round SHALL earn `beat_factor × c` units, `c` its delta changes; a rewrite SHALL cost its input tiles'
entries, its delta changes and its outputs' entries; at each boundary the beat SHALL spend the units on rewrites in
progress and then on the queue's head in queue order, discarding units left when nothing is queued or in progress
(`STORAGE.md` section 16.4). While a rewrite of reason 5 is queued or in progress, every rewrite in progress or queued
ahead of it SHALL complete at that boundary. A rewrite completing at a boundary whose input tiles are not yet read SHALL
hold that boundary until they are, so no I/O timing changes what is written or where.

#### Scenario: Completion order changes nothing written
- **WHEN** two engines apply the rounds of `engine.json` with different I/O completion orders and cache sizes
- **THEN** both reproduce the run's `events` and `checkpoints`

### Requirement: Drops and purges are logged markers that reads honour and compaction removes
A purge's round SHALL write a purge marker (kind 20) per instance it names, holding the purge's version, in every
partition holding entries naming it; reads at every version SHALL treat every entry naming the instance as absent, and
compaction SHALL remove them, class I included (`STORAGE.md` section 15.5; `PurgeAbsent`). A drop SHALL write a drop
marker (kind 14); reads at every version SHALL treat the entries it covers as absent and compaction SHALL remove them; a
graph drop SHALL remove the graph's families from the directory at its round. A tile a drop marker or purge covers SHALL
be due under reason 4, and every family a drop marker covers SHALL be examined and due at its round's boundary, so its
tiles are rewritten to nothing and released (`STORAGE.md` sections 15.5 and 16.1; T48). A catalog version taking effect
(`PROTOCOL.md` section 12.2, operation 6) that removes a tokenizer of an index, and starts no build of that
index whose swap removes it, SHALL write in its round a drop marker naming operation 6 and the version, covering that
tokenizer's index families (T48).

#### Scenario: Compaction removes a purged class I entry
- **WHEN** an `@append_only` instance is purged and its family's tile is rewritten
- **THEN** no output holds an entry naming the instance, and conservation counts each as removed

#### Scenario: A dropped predicate reads as absent at once
- **WHEN** predicate `p` is dropped at version 30 and a read at 20 follows before any rewrite
- **THEN** the read finds no entry of `p`

#### Scenario: A dropped predicate's tiles are released
- **WHEN** predicate `p`, whose families hold tiles and no delta change, is dropped in a round
- **THEN** every tile of `p`'s families is due at that round's boundary, each rewrite writes no output, and the closed extents
  holding only those tiles are released

### Requirement: Index builds run beside compaction and resume from their record
A build record (kind 15) SHALL start an index build of the generic layout at its position. The build SHALL walk its
source family in key order, `beat_factor × tile_entries_max` entries per boundary, drawing no compaction unit, reading
its source at its start version and writing index entries as compaction outputs created at that version; changes
committed meanwhile SHALL reach the index through apply (`STORAGE.md` section 16.7). Its record SHALL carry the last key
reached, updated at each checkpoint; a recovering replica SHALL resume strictly after the key the checkpoint holds. Its
completion SHALL be a logged operation, so every replica finishes at one position. Governed tenants SHALL build no index
before stage 9.5.

#### Scenario: A build resumes after a crash
- **WHEN** a replica crashes during a build whose record at the last checkpoint names key `k`
- **THEN** recovery resumes the build strictly after `k`, and the index it finishes equals that of a replica that did
  not crash

#### Scenario: A write during a build reaches the index
- **WHEN** a statement of the source predicate is committed while its build runs
- **THEN** after the build's logged completion the index holds the statement's tokens

### Requirement: A bucket group's move keeps its tiles' bytes
A move of a bucket group SHALL be a logged operation at a position of the source's log, specified here and built in
stage 8 (`STORAGE.md` section 16.8). At its boundary the source SHALL make every family of the group due under reason 1
and rewrite them before any other, each completing at that boundary, send the group's live tiles with their bytes
unchanged, its slot maps and identity columns among them, and drop them at the move's position, keeping copied
dictionary entries and purge markers of `group` `ABSENT`. The destination SHALL place each tile in stream 2 in tile-key
order without changing a byte, and a rewrite whose first input was moved in SHALL stamp `previous` 0.

#### Scenario: A moved group keeps its slots
- **WHEN** stage 8 moves a bucket group from a source partition to a destination
- **THEN** the destination's installed tiles hold the bytes the source sent, the source's directory no longer holds
  them, and the group's slot maps give every entity its slot of before
