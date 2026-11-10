## ADDED Requirements

### Requirement: The derivation version
The derivation version SHALL be the XXH3-64 of 148 bytes: the u16 format 1, the u16 algorithm 1, then the eighteen
settings of the `[tiles]` table as u64s in the table's order, as `specification/storage/STORAGE.md` sections 3.1 and
3.2 state. A replica whose settings give another version than its superblock's SHALL refuse to open with
`DerivationMismatch`, and replay SHALL refuse a round naming another derivation version with `DerivationMismatch`.

#### Scenario: The derivation vectors
- **WHEN** the derivation version is computed from section 3.1's defaults and from `derivation.small_grid.settings`
  of `vectors/grid.json`
- **THEN** it equals `derivation.defaults`, `e0f11e730ef7e4ac`, and `derivation.small_grid.value` respectively

#### Scenario: A changed setting refuses the open
- **WHEN** a grid whose superblock holds the default derivation version is opened with `checkpoint_bytes` doubled
- **THEN** the open is refused with `DerivationMismatch`, and nothing is replayed

### Requirement: The grid file
A replica's grid SHALL be the file `grid` in `<data_dir>/p{partition}/r{replica}/`, of exactly
`extent_bytes × grid_extents` bytes, created whole and zeroed at first open, with `fallocate` where the file system
supports it, and never resized; on Linux it SHALL be opened with `O_DIRECT`, and every read and write SHALL be a whole
number of 4,096-byte blocks at a block boundary, as `specification/storage/STORAGE.md` sections 12.1 and 17.3 state.
Extent 0 SHALL hold the superblock's four copies in blocks 0 to 3, its other blocks zero and never written; extents 1
to `grid_extents − 1` hold tiles, each starting at a block boundary with the bytes from its end to the next boundary
zero (sections 6.1 and 12.2). A new grid SHALL hold no checkpoint: its recovery replays the log from position 1, and
its first checkpoint, sequence 1 with `parent` 0, is the first section 17.1 makes.

#### Scenario: A first open creates the whole grid
- **WHEN** a replica opens a data directory holding no grid, with 64 extents of 131,072 bytes
- **THEN** the file `grid` holds exactly 8,388,608 zero bytes, and the replica replays its log from position 1

#### Scenario: The first checkpoint of a new grid
- **WHEN** the rounds of `vectors/engine.json` are applied to a new grid
- **THEN** its first checkpoint, at position 15, has `sequence` 1 and `parent` 0

#### Scenario: A tile's last block is zero-padded
- **WHEN** a tile of 20,000 bytes is placed
- **THEN** it takes 5 blocks, and the 480 bytes after its end are zero

### Requirement: The superblock
The superblock SHALL be laid out as `specification/storage/STORAGE.md` section 12.3 states and written as four copies,
copy `i` in block `i` holding `copy` `i`, its `checksum` the XXH3-64 of the 4,096 bytes with `copy` and `checksum`
zeroed, so the four copies hold one checksum; `parent` the previous checkpoint's `checksum`, 0 for the first;
`sequence` one more than the previous; `log_bytes` the Round frames' total length through `position`. The superblock
SHALL be authoritative for the store's state; the WAL's own superblock records its segments, the position and checksum
of the last round it trimmed and the parent of the first round it keeps, and nothing of the store's state.

#### Scenario: The superblock vector
- **WHEN** the superblock of `superblock.input` in `vectors/grid.json` is encoded
- **THEN** its four copies equal `superblock_copy0.block` to `superblock_copy3.block`, each holds the checksum
  `superblock.checksum`, and each decodes to `superblock.input`

### Requirement: Extents and streams
At every moment an extent other than extent 0 SHALL be free, open in one of three streams, closed, or released, as
`specification/storage/STORAGE.md` section 12.4 states. Stream 0 holds manifest tiles and the free set, stream 1 hot
data tiles and stream 2 cold data tiles: a data tile SHALL go to stream 2 when `previous` is nonzero and
`built − previous ≥ cold_after_rounds`, and to stream 1 otherwise; a tile the cleaner moves SHALL go to stream 2.

#### Scenario: The stream vectors
- **WHEN** each case of `streams` in `vectors/decisions.json` is decided
- **THEN** its stream is the case's `stream`: a tile replacing none goes to stream 1, and of two tiles built at 70,000
  under `cold_after_rounds` 65,536, the one whose predecessor was built at 4,464 goes to stream 2 and the one at 4,465
  to stream 1

### Requirement: Placement takes the lowest free extent
A tile of `b = blocks(length)` blocks placed in stream `s` SHALL go at the cursor of `s`'s open extent when the cursor
plus `b` is at most `extent_bytes / 4,096`, advancing the cursor by `b`; otherwise the open extent, if any, SHALL
close, and the lowest-numbered free extent SHALL open in `s` with its cursor at 0 and take the tile, as
`specification/storage/STORAGE.md` section 12.4 states. With no free extent the store SHALL be full (`GridFull`).

#### Scenario: The allocation script
- **WHEN** the placements of `allocation` in `vectors/grid.json` are made in order on 64 extents of 131,072 bytes
- **THEN** each lands at its `address`

#### Scenario: Closing an extent and opening the lowest free
- **WHEN** the script's third tile, 13 blocks in stream 1, meets extent 1 with its cursor at block 20 of 32
- **THEN** extent 1 closes, extent 2 opens in stream 1 and takes the tile at 262,144; the next stream 2 tile opens
  extent 3; and the 8-byte tile after it lands at extent 2's cursor, 315,392

### Requirement: Release, reuse and punching
Each closed or open extent SHALL count the blocks of its live tiles: the data tiles the directory references and the
manifest leaves, root and free set of the latest checkpoint. A closed extent whose live count reaches 0 between
checkpoints `N` and `N + 1` SHALL become released, stay marked allocated in checkpoint `N + 1`'s free set, and become
free at the boundary of the first round after checkpoint `N + 1`'s position, before that boundary's placements, which
SHALL wait for step 6 of the checkpoint when it has not completed, as `specification/storage/STORAGE.md` sections 12.4
and 17.1 state. An open extent SHALL never be released. A freed extent's blocks SHALL be punched with
`FALLOC_FL_PUNCH_HOLE | FALLOC_FL_KEEP_SIZE` where the file system supports it; where it does not, the bytes stay and
SHALL never be read again.

#### Scenario: Extents freed by a checkpoint are reused after it
- **WHEN** the rounds of `vectors/engine.json` reach the checkpoint at position 25
- **THEN** its free set marks extents 0 to 4, its step 6 frees extents 1 and 2, and round 26's boundary finds them
  free before its placements

#### Scenario: A released extent waits for the next checkpoint
- **WHEN** an extent's last live tile is dropped from the directory between two checkpoints
- **THEN** no placement takes the extent until the boundary of the first round after the next checkpoint's position,
  and that checkpoint's free set marks it

### Requirement: The reserve
`free` SHALL count the free extents, a released extent counting once it is freed, and the reserve SHALL be what the
next checkpoint can place at most, the drain's rewrites of every tile holding delta changes, then the manifest and the
free set, by the formula of `specification/storage/STORAGE.md` section 12.4, a change counting while it is in the
delta whether or not a rewrite in progress has taken it.

#### Scenario: The reserve vectors
- **WHEN** each case of `reserve` in `vectors/decisions.json` is computed from its settings, tiles, manifest blocks and
  free set blocks
- **THEN** it equals the case's `reserve`: 4 with no delta change and no checkpoint yet, 18 for forty full tiles with
  ten changes each

### Requirement: GridFull and its logged checkpoint round
While `free` is below the reserve, `GridFull` SHALL hold the partition and admit no commit, as
`specification/storage/STORAGE.md` sections 12.4 and 19.4 state. A primary whose partition `GridFull` holds SHALL, when
the delta holds changes or an extent has been released since the last checkpoint, log a round flagged `checkpoint`
(`PROTOCOL.md` section 12.1, `flags` bit 0), and SHALL log the next round at once, empty when admission takes no entry,
so that checkpoint's frees take effect at that round's boundary. When the delta holds no change and no extent has been
released since the last checkpoint, `GridFull` SHALL persist and refuse every commit with code 5 (Busy), `detail` 2,
until a larger `grid_extents`, and so a new derivation version and a new grid, relieves it. A manifest root that would
exceed `tile_bytes_max` SHALL be that persistent `GridFull`, code 5 `detail` 2, relieved only by a larger
`tile_bytes_max` or grid in a new derivation version (section 13.3; T48).

#### Scenario: A full grid leaves by its logged checkpoint
- **WHEN** `free` falls below the reserve while the delta holds changes
- **THEN** the primary logs a round flagged `checkpoint` and the next round at once, the store checkpoints at the
  flagged round with its drain, and at the next round's boundary the released extents are free and `free` is at or
  above the reserve

#### Scenario: A grid holding only live data stays full
- **WHEN** `free` is below the reserve, the delta holds no change and no extent has been released since the last
  checkpoint
- **THEN** no flagged round is logged, and every commit is refused with code 5, `detail` 2

### Requirement: Tile keys and families
A tile key SHALL be the bytes `shape`, `kind`, `tenant` (u64 big-endian), `graph`, `subject`, `type` (u16 big-endian),
`direction`, `class` (1 when `flags` holds `class_i`, else 0), `tokenizer` and `group` (u32 big-endian), then the
fence: an adjacency block's first row key and its first entry's neighbour joined as section 5.4 states, a column
chunk's base slot as a big-endian u32, a record run's first key as `esc` of it, nothing for the free set, as
`specification/storage/STORAGE.md` section 13.1 states. The 51 bytes before the fence SHALL be the family key, and a
family SHALL be the tiles sharing it.

#### Scenario: The tile key vectors
- **WHEN** the tile key of each tile of `vectors/tiles.json` and `vectors/kinds.json` that names one is computed from
  its header and entries
- **THEN** it equals `tile_key_hex`, its fence equals `fence_hex`, and its first 51 bytes are its family key

#### Scenario: The reference engine's families
- **WHEN** the family keys of the forward and reverse families of `vectors/engine.json` are computed
- **THEN** they equal `families.forward.family_key` and `families.reverse.family_key`

### Requirement: The directory
The directory SHALL be the in-memory map from tile key to reference and entries of every live data tile, rebuilt at
open from the manifest. An adjacency entry or a record, its (row key, neighbour) or its key joined as a fence is, SHALL
belong to the tile of its family with the greatest fence at or below it, or to the family's first tile when it
precedes every fence; a family with no tile holds nothing, and its first entry makes a new tile; a column chunk's slot
belongs to its chunk as section 8.2 states, as `specification/storage/STORAGE.md` section 13.2 states.

#### Scenario: The lookup vectors
- **WHEN** each case of `lookups.cases` in `vectors/decisions.json` is looked up among `lookups.fences_hex`
- **THEN** it belongs to the case's `tile`

#### Scenario: A key below every fence
- **WHEN** the case whose key precedes the first fence is looked up
- **THEN** it belongs to tile 0, the family's first

### Requirement: The manifest
The manifest SHALL be the directory on disk in two levels of record runs: leaves of kind 16, the directory's entries
in tile-key order, cut greedily so that no leaf holds more than `tile_entries_max` records or encodes to more than
`tile_bytes_max` bytes, and one root of kind 17 holding each leaf's first key and reference, as
`specification/storage/STORAGE.md` section 13.3 states. Each checkpoint SHALL cut the leaves afresh from the whole
directory; a leaf equal, byte for byte apart from `built`, `previous`, `horizon` and `checksum`, to the previous
checkpoint's leaf
at the same position in the order SHALL keep that leaf's reference and not be written; the root SHALL always be
written.

#### Scenario: The manifest vectors
- **WHEN** the leaf and root of `vectors/tiles.json` are encoded for the directory its data tiles form
- **THEN** they equal `manifest_leaf.tile` and `manifest_root.tile` and land at their `address`

#### Scenario: An unchanged leaf is not written again
- **WHEN** a checkpoint follows one whose directory's entries of a leaf have not changed
- **THEN** the leaf keeps its reference and is not written, and a new root is written

### Requirement: Placement order within a boundary
Within one boundary the store SHALL place, in turn: the outputs of rewrites in progress in the order they started, then
of queued rewrites in queue order, each rewrite's outputs in tile-key order; at a checkpoint, the drain's rewrites; the
cleaner's moves; then a checkpoint's manifest leaves, root and free set, as `specification/storage/STORAGE.md` section
16.5 states.

#### Scenario: Data tiles before the checkpoint's tiles
- **WHEN** the data tiles of `vectors/tiles.json` are placed in `built` order, then the leaf, the root and the free set
  in stream 0
- **THEN** each lands at its `address`, and the checkpoint's free set marks extents 0, 1 and 2, as
  `checkpoint.extents_allocated` of `vectors/grid.json` states

### Requirement: The cleaner
The cleaner SHALL be due at a round boundary with `free × 100 < cleaner_free_percent × grid_extents` or `free` below
twice the reserve. When due, it SHALL take the closed extent with the fewest live blocks, the lowest-numbered among
equals, among those holding no input of a rewrite in progress and no stream 0 tile, and move each of its live tiles, in
address order, to stream 2, bytes unchanged and only the reference's address new; it SHALL move one extent per
boundary, after the boundary's rewrites have placed their outputs, only while `free` is at least the reserve plus 2,
and the extent becomes released; at a checkpoint boundary it SHALL run once, after the drain, as
`specification/storage/STORAGE.md` sections 12.4, 16.6 and 17.1 state.

#### Scenario: The cleaner vectors
- **WHEN** each case of `cleaner` in `vectors/decisions.json` is decided
- **THEN** `due` and `moves` are the case's: 9 free of 64 against a reserve of 4 is due and moves; 20 free against a
  reserve of 11 is due, below twice the reserve, and moves; 5 free against a reserve of 4 is due and does not move

#### Scenario: A moved tile keeps its bytes
- **WHEN** the cleaner moves a tile
- **THEN** the tile's bytes at its new address equal those at its old one, header included, and the directory names
  the new address with the same length and checksum

#### Scenario: Equal extents go lowest first
- **WHEN** two closed extents hold the fewest live blocks, equally
- **THEN** the cleaner takes the lower-numbered

### Requirement: Checkpoint positions
The first checkpoint SHALL be due at the first round boundary `p` with `T(p) ≥ checkpoint_bytes`, `T(p)` the total
length of the Round frames at positions 1 to `p`; after a checkpoint at `q`, the next at the first boundary `p` with
`T(p) ≥ (T(q) div checkpoint_bytes + 1) × checkpoint_bytes`, so a round crossing several multiples makes one
checkpoint; a round flagged `checkpoint` SHALL be a checkpoint position too, as `specification/storage/STORAGE.md`
section 17.1 states. Checkpoint positions SHALL be a function of the log alone. A clean shutdown SHALL sync the log and
apply the rounds logged and not yet applied, taking every checkpoint whose position they reach, as at any time, and
SHALL add none of its own (T48).

#### Scenario: The checkpoint position vectors
- **WHEN** each case of `checkpoints` in `vectors/decisions.json` is decided from its frame lengths, flags and
  `checkpoint_bytes`
- **THEN** its positions are the case's: 4, 8, 11 and 13 with no round flagged; 2, 4, 8, 11, 12 and 13 with rounds 2
  and 12 flagged

#### Scenario: A flagged round already due makes one checkpoint
- **WHEN** the case `round 4 flagged, already a checkpoint by its bytes` is decided
- **THEN** its positions are 4, 8, 11 and 13, the same as with no round flagged

#### Scenario: A clean shutdown writes no checkpoint of its own
- **WHEN** a replica shuts down cleanly between checkpoints, the rounds it still applies reaching no checkpoint position,
  and is reopened
- **THEN** its superblock is unchanged and it replays from its last checkpoint's position

### Requirement: A checkpoint drains, publishes and frees
A checkpoint SHALL, after the boundary's rewrites have placed their outputs, take the steps of
`specification/storage/STORAGE.md` section 17.1 in order: 0, drain the delta, completing every rewrite in progress or
queued in queue order and then rewriting every tile still holding delta changes in tile-key order, this boundary
deciding, then run the cleaner once if due; 1, close every open extent; 2, cut the manifest and place the leaves to be
written, the root and the free set in stream 0, then encode the free set marking every extent not free after these
placements, the extents released since the last checkpoint included, then close the extents these placements opened;
3, sync the grid; 4, write superblock copies 0, 1, 2 and 3 in turn, syncing after each; 5, write the WAL's superblock
and trim the WAL through `position`; 6, free and punch every released extent and every closed extent holding no live
tile. After step 0 the delta SHALL be empty, no rewrite in progress, the queue empty and the units held 0.

#### Scenario: The drain empties the delta
- **WHEN** a checkpoint position is reached while rewrites are queued and in progress and other tiles hold delta changes
- **THEN** every one completes at that boundary, and after step 0 the delta, the queue and the units are empty

#### Scenario: The I/O of a checkpoint in order
- **WHEN** the grid's sans-I/O core makes a checkpoint
- **THEN** its requests are the writes of its stream 0 tiles, one grid sync covering them and every data tile placed
  since the last checkpoint, then for each copy 0 to 3 its write followed by a sync, then the WAL's superblock and its
  trim, then the punches of the freed extents

#### Scenario: The reference engine's checkpoints
- **WHEN** the rounds of `vectors/engine.json` are applied
- **THEN** each checkpoint's position, `log_bytes`, `sequence`, superblock checksum, identity, tile and leaf counts,
  marked extents and freed extents equal the entry of `checkpoints`

### Requirement: The crash order
The crash order SHALL be the steps' order: a crash before step 4 leaves the previous superblock in force; a crash
during step 4 leaves copies of two sequences, of which recovery takes the newer whose references verify; after step 4
the new checkpoint is in force whatever the WAL's state, as `specification/storage/STORAGE.md` section 17.1 states. A
replica recovering from the previous checkpoint SHALL replay to the new one's position and write it again, the same
bytes at the same addresses.

#### Scenario: A crash before the superblock
- **WHEN** a replica crashes after step 3 of a checkpoint and is reopened
- **THEN** the previous checkpoint is in force, and replay writes the lost checkpoint's tiles and superblock with the
  same bytes at the same addresses

#### Scenario: A crash between copies
- **WHEN** the case `crash during step 4: copies 0 and 1 new` of `superblock_selection` in `vectors/decisions.json` is
  selected
- **THEN** recovery takes sequence 5

### Requirement: Superblock selection
At open the store SHALL read blocks 0 to 3, keep each copy whose `magic`, `format`, `copy` (its block's number) and
`checksum` hold, and take the highest `sequence` among them, as `specification/storage/STORAGE.md` section 17.2 steps
1 and 2 state. Two kept copies of one `sequence` differing in any byte but `copy` SHALL refuse the open
(`SuperblockDiverged`); with none kept, an all-zero extent 0 SHALL be a new grid and anything else SHALL refuse the
open (`SuperblockLost`); when the manifest or free set the highest sequence references fails its checks and a kept copy
of the sequence below holds, recovery SHALL take that one, and with none SHALL refuse the open naming the failing tile's
key and reference (T48).

#### Scenario: The selection vectors
- **WHEN** each case of `superblock_selection` in `vectors/decisions.json` is selected
- **THEN** the copies kept are the case's `kept` and the result is its `result`: sequence 5, `SuperblockDiverged`,
  a new grid or `SuperblockLost`

#### Scenario: Broken copies are not kept
- **WHEN** the cases `superblock with a flipped bit` and `superblock copy field other than its block` of
  `vectors/refusals.json` are read
- **THEN** they are refused with `Checksum` and `Copy` respectively, and neither copy is kept

#### Scenario: References that fail fall back a sequence
- **WHEN** copies 0 and 1 hold sequence 6, whose manifest root fails its checks, and copies 2 and 3 hold sequence 5
- **THEN** recovery takes sequence 5

### Requirement: Recovery rebuilds from the superblock and replays
Recovery SHALL refuse the chosen superblock with `DerivationMismatch` for a `derivation`, `extent_bytes` or
`grid_extents` other than the replica's settings give, and with `Identity` for a `cluster`, `partition` or `replica`
other than the replica's own and for an `identity` that does not recompute by section 11.3 (T48); rebuild, from the
free set and the manifest superblock selection checked, the directory, the
live block counts and the extents' states, an extent the free set marks and holding live blocks closed, every other
extent free and none open; then start with an empty delta, an empty queue and no units and replay the log from
`position + 1`, as `specification/storage/STORAGE.md` section 17.2 steps 3 to 5 state, its checks in the order that
section gives. A data tile failing its checks during recovery SHALL be repaired before the open continues, and one with
no peer to repair it from, as at stage 6.5's one replica, SHALL stop the replica with the refusal naming its tile key
and reference.

#### Scenario: Another replica's grid is refused
- **WHEN** replica 2 opens a grid whose superblock names replica 1
- **THEN** the open is refused with `Identity`

#### Scenario: A marked extent with no live tile is free after recovery
- **WHEN** a replica crashes after step 4 of a checkpoint and before step 6, and recovers
- **THEN** every extent the free set marks that holds no live tile is free, as on a replica that did not crash, and no
  extent is open

#### Scenario: An unrepairable manifest leaf refuses the open
- **WHEN** a manifest leaf the highest sequence references fails its checksum and no kept copy of the sequence below
  holds
- **THEN** the open is refused naming the leaf's tile key and reference

### Requirement: Repair
A replica that finds a tile failing its checks, at a read, at recovery or while scrubbing, SHALL ask a peer for the
block range its reference names, by address and checksum, and accept only bytes whose checksum is the reference's;
before stage 8, or with no peer holding them, a tile written after the last checkpoint SHALL be rebuilt by replaying
the log, and any other is unrepairable: the replica stops serving, naming the tile, as
`specification/storage/STORAGE.md` section 17.4 states. Repair SHALL write only the bytes its reference names. The
peer exchange is specified here and built in stage 8.

#### Scenario: A tile written after the checkpoint is rebuilt
- **WHEN** a data tile placed since the last checkpoint fails its checksum on a single replica
- **THEN** replaying the log from the checkpoint rewrites it with the bytes its reference names, at its address

#### Scenario: A checkpointed tile with no peer stops the replica
- **WHEN** a tile the last checkpoint references fails its checksum and no peer holds it
- **THEN** the replica stops serving, naming the tile

### Requirement: Scrubbing
A scrub SHALL read every live tile once per cycle, in address order, `beat_factor × tile_entries_max` entries' worth of
tiles per boundary, checking each as a reader does and its digest against its entries, and SHALL repair a tile that
fails, as `specification/storage/STORAGE.md` sections 17.4 and 17.5 state.

#### Scenario: A flipped bit at rest is found within a cycle
- **WHEN** one bit of a live tile is flipped at rest and a scrub cycle runs
- **THEN** the scrub finds the tile, it is repaired, and no boundary of the cycle scrubs more than
  `beat_factor × tile_entries_max` entries' worth of tiles

### Requirement: State sync adopts a peer's checkpoint
A replica adopting a peer's checkpoint by state sync SHALL take its tiles at their addresses and write its own
superblock with its own `replica`, its next `sequence` and its last superblock's `checksum` as `parent`, every other
field the peer's, so the checkpoint identity is the peer's, as `specification/storage/STORAGE.md` section 17.6 states.
State sync is specified here and built in stage 8.

#### Scenario: An adopted checkpoint keeps the peer's identity
- **WHEN** replica 2, whose last superblock has sequence 3, adopts replica 1's checkpoint of sequence 7
- **THEN** replica 2's superblock names replica 2, sequence 4 and its own last checksum as `parent`, every other field
  replica 1's, and both superblocks hold one identity
