## ADDED Requirements

### Requirement: The engine is owned by its partition's core
Each partition's engine SHALL be owned by the core whose runtime runs the partition, and every step that reads or
changes it, apply, compaction, a checkpoint step, a read and an execution, SHALL run on that core's thread, the core
passing the engine into the step (`specification/storage/STORAGE.md` section 2). A transaction, a Scan, a Frontier
request and parked work SHALL each own their state, a read version, a write overlay, a read set and pending terms, and
SHALL hold handles into the engine and no borrow of it across turns. The engine SHALL hold no lock and no atomic on the
data plane; other threads SHALL reach it only by messages.

#### Scenario: A transaction spans turns while rounds apply
- **WHEN** a read-write transaction reads `name` of `a` in one turn, a round committing a new `name` of `a` applies in a
  later turn, and the transaction reads `name` of `a` again
- **THEN** both reads answer the value at the transaction's read version, and the round applied without waiting for the
  transaction

#### Scenario: Two partitions' engines share no state
- **WHEN** one storage process hosts partitions 1 and 2 on two cores and both apply rounds at once
- **THEN** each engine is changed only by its own core's thread, and neither partition's apply waits on the other's

### Requirement: An attempt that misses the cache runs again from its request
An attempt, a read or a write's execution, that needs a tile not in the cache SHALL return `Miss` with the references of
the tiles it needs; its side effects SHALL be undone, an ID it allocated released among them, it SHALL park, the tile
reads SHALL be issued in its turn's I/O submission, and it SHALL run again from its request once they complete
(`STORAGE.md` section 18.2; T33). Nothing an attempt did before its `Miss` SHALL be visible to any other attempt or
reach the log.

#### Scenario: A read missing a tile answers after it loads
- **WHEN** a transaction reads `name` of `a`, whose tile is not in the cache
- **THEN** the read parks, the tile is read in the turn's submission, and the read is run again from its request and
  answers `a`'s `name` at the transaction's read version

#### Scenario: A write's execution undone on a miss allocates once
- **WHEN** a write creating `<new>` allocates its ID and then misses a tile
- **THEN** the allocation is released, and the write run again allocates `<new>`'s ID once and commits it

### Requirement: Reads see two logical overlays
A transaction's own writes SHALL be held as a logical write overlay, named by system IDs and logical values, never by
slots or tiles, and every lane X read of the transaction SHALL merge it over the tile and delta entries visible at its
read version (storage-lanes, "A read merges tiles, the delta and its own writes"). Commit-time operations SHALL read the
partition's latest applied state with the overlay of the proposals the commit step admitted but has not yet applied
merged over it, so an operation decided in a round sees every operation and write admitted before it; that overlay
SHALL hold each proposal until its round applies.

#### Scenario: A counter add sees an admitted add not yet applied
- **WHEN** the counter `next-id` holds 5, and two transactions each add 1 to it, the second admitted while the first's
  round is still syncing
- **THEN** the first receives 6 and the second 7, and `next-id` holds 7 once both rounds apply

#### Scenario: An admitted proposal leaves the overlay when its round applies
- **WHEN** a round holding an insert-if-absent's created node is applied
- **THEN** the admitted-proposal overlay drops it, and a later operation reads the node from the engine

### Requirement: The commit path writes logical conflict keys
A transaction's write-set record SHALL name what it writes as conflict keys of the eight conflict families of
`PROTOCOL.md` section 12.2, forward row, reverse row, index token, count, statement, mark, claim and session, each laid
out as that section states: its conflict points, its conflict statements with the neighbour each changes in its logical
form, and a drop's conflict ranges. No conflict key
SHALL name a tile, a family of the store or a slot, so a tile's split, rewrite or move changes no conflict. A drop's
conflict ranges SHALL be the tenant's range for a tenant's data and for everything in a tenant, the tenant's and the
graph's for a graph, and for a predicate the tenant's, each graph's holding it in the partition and the predicate's.

#### Scenario: A rewrite of a tile changes no conflict
- **WHEN** a transaction reads `knows` of `a` in lane X, the tile holding that row is rewritten and split, and another
  transaction then commits `<a> <knows> <c>`
- **THEN** the first transaction's commit conflicts exactly as it would without the rewrite

#### Scenario: A predicate drop is one range per graph
- **WHEN** `p` is held in graphs `g1` and `g2` of a partition and is dropped
- **THEN** the drop's record holds two conflict ranges, one of `g1` and `p` and one of `g2` and `p`, and a transaction
  that read `p` in `g2` at a read version below the drop's and commits after it aborts with a conflict of kind Drop

## MODIFIED Requirements

### Requirement: A request reads its own earlier edges
Within one transaction, applying an edge SHALL read its row with that transaction's earlier writes, held in its write
overlay, merged over the state at its read version; no other transaction SHALL see them before the transaction is
acknowledged, and a transaction SHALL never read another transaction's writes that are not visible at its read version.
A row the transaction wrote without reading it SHALL be read whole, its tile and delta entries visible at the read
version merged with the overlay, before any read of it in the same transaction answers.

#### Scenario: A later edge reads an earlier edge of its request
- **WHEN** `name` of `0x1` holds no value and a transaction sets it to `A` and then deletes `A`
- **THEN** after the commit, `name` of `0x1` holds no value

#### Scenario: Another request does not see uncommitted writes
- **WHEN** one transaction has set `name` on `0x1` and not committed, and a second transaction reads it
- **THEN** the second transaction sees the value before the first transaction's write

#### Scenario: A transaction in the same round does not read another's writes
- **WHEN** two transactions of one round execute from the same watermark and the first sets `name` of `0x1` to `A`
- **THEN** the second reads `name` of `0x1` as it was at the watermark

#### Scenario: A row first written blind is read whole
- **WHEN** `friend` of `0x1` holds `0x2` in the store, and a transaction adds `0x3` without reading the row and then
  reads it
- **THEN** the read holds `0x2` and `0x3`

### Requirement: A commit is written in one batch at its timestamp
The admitted transactions of a round SHALL be applied, once the round is synced in the log, as one step of the
partition's engine turning the round into delta changes, every entry's changes and the index, reverse, count and record
changes they derive, each transaction's at its commit version, without a sync of the grid (tile-engine, "Apply reads the
round and adds"). The applied position and the visibility watermark SHALL advance only after the whole round is applied.
The grid SHALL be synced at checkpoints alone (write-ahead-log, "The log is the only synced stream"). A transaction
SHALL take its commit version from the commit step.

#### Scenario: A commit is visible whole
- **WHEN** a transaction writing three rows commits at 12 while another reader reads at 12 repeatedly
- **THEN** every read sees the changes to all three rows or to none of them

#### Scenario: Two commits in one batch are visible together
- **WHEN** one round holds a commit at 12 and a commit at 14 while another reader reads at 14 repeatedly
- **THEN** every read sees both commits or neither

#### Scenario: A failed request writes nothing
- **WHEN** a transaction aborts after building writes
- **THEN** the store holds none of its writes

#### Scenario: A failed request holds up no read
- **WHEN** a transaction aborts and another transaction begins
- **THEN** the second transaction begins at the watermark without waiting for the aborted one

### Requirement: Applied timestamps gate reads
A transaction SHALL read at the visibility watermark, which never passes a version not yet synced and applied, so its
reads never wait for a commit; a read at an explicit version above the watermark SHALL follow `PROTOCOL.md` section
13.13: at or below the version bound of an epoch the partition has closed it reads what the partition applied there,
and any other is refused with code 27 (AheadOfWatermark), `detail` the watermark, at once with `at` 4 and with `at` 1
once the partition has closed the Scan's `epoch`. Compaction SHALL take no version from the commit step and SHALL fold only at the fold horizon its round logged, which
the read version of every open read-write transaction and the version of every open lane S request bound on the primary
(tile-engine, "The fold horizon is logged by the primary and folded at by every replica"), so no open read loses a
version it reads.

#### Scenario: A read above the watermark answers by its epoch
- **WHEN** a commit at 10 is being applied, the watermark below 10, and one Scan reads at 10 with `at` 1 naming 10's
  epoch while another reads at 10 with `at` 4
- **THEN** the first answers once the partition has closed the epoch and sees the commit, and the second is refused at
  once with code 27, `detail` the watermark

#### Scenario: A rewrite leaves an open read intact
- **WHEN** a read at 6 is open and a tile holding an entry created at 6 and deleted at 8 is rewritten
- **THEN** every round logged while the read is open carries a horizon at or below 6, the rewrite keeps the entry with
  its versions, and the read at 6 still returns it
