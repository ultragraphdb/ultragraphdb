## ADDED Requirements

### Requirement: The simulator performs the engine's I/O under faults
The simulator SHALL run the engine with its I/O requests performed by a model grid, as
`specification/storage/STORAGE.md` sections 2 and 19.5 state: the engine's inputs are rounds, the catalog versions
they name, read requests, I/O completions and epoch closes, and its outputs are answers and I/O requests, so the
simulator and the compio shell drive the same engine code. The simulator SHALL check, after every step, the shadow
model, every invariant of section 19.3, replica identity and the response to every injected fault.

#### Scenario: One engine under two shells
- **WHEN** one log is applied by the engine under the compio shell and under the simulator with no fault injected
- **THEN** both write the same tiles at the same addresses and the same superblocks

### Requirement: A run is a function of its seed
A simulation run SHALL be a function of its seed: the simulator's clock, its random source, the faults it injects and
the order in which I/O completes SHALL all derive from the seed, as `specification/storage/STORAGE.md` section 19.5
states.

#### Scenario: A seed replays its run
- **WHEN** one seed is run twice
- **THEN** both runs inject the same faults at the same steps, complete I/O in the same order and end with the same
  grids and the same answers

### Requirement: Every fault class is injected
The model grid SHALL inject every fault of `specification/storage/STORAGE.md` section 19.1: a crash at any
instruction; power loss at any moment; a write torn at any 512-byte boundary; a write the drive acknowledged and lost;
a write or a read misdirected to another block; a latent sector error; bits flipped at rest; a failed sync, after which
nothing the sync covered is trusted; and a punch the file system refuses.

#### Scenario: A torn write damages nothing durable
- **WHEN** a tile write is torn at a 512-byte boundary and the replica then crashes before its next checkpoint's step 4
- **THEN** recovery from the grid alone succeeds, since no durable superblock references the torn bytes, and replay
  writes the tile again at the same address with the same bytes

#### Scenario: Power loss loses unsynced tile writes
- **WHEN** power is lost after data tiles were written and before the grid sync of the next checkpoint
- **THEN** recovery opens at the previous checkpoint and replay rewrites those tiles byte for byte

### Requirement: Damaged tile bytes are repaired
Tile bytes that are torn, lost or flipped SHALL be detected by the tile's checksum and repaired as
`specification/storage/STORAGE.md` section 17.4 states, as section 19.2's table gives the response.

#### Scenario: A flipped bit in a tile written since the checkpoint
- **WHEN** a bit is flipped at rest in a data tile placed after the last checkpoint and the tile is read
- **THEN** the read refuses the tile with `Checksum`, and the tile is rebuilt by replaying the log before any answer
  uses it

#### Scenario: A latent sector error under a checkpointed tile
- **WHEN** a read of a tile the last checkpoint references fails with a latent sector error and no peer holds the tile
- **THEN** the replica stops serving, naming the tile

### Requirement: Misdirected and stale tiles are repaired
A misdirected or stale tile SHALL be detected by the reference's checksum and by the header's family and fence, and
repaired, as `specification/storage/STORAGE.md` sections 6.4 and 19.2 state.

#### Scenario: A read misdirected to another tile
- **WHEN** a read of one tile is misdirected to the blocks of another valid tile
- **THEN** the decoder refuses it with `Misdirected` or `Length`, nothing of it is used, and the tile is repaired

#### Scenario: A lost overwrite leaves a stale tile
- **WHEN** a write placing a tile into a freed extent is acknowledged and lost, leaving that extent's earlier tile
  bytes, and the new tile is read through its reference
- **THEN** the read refuses the stale bytes and the tile is repaired

### Requirement: A torn superblock copy falls back to the others
A torn superblock copy SHALL be detected by its checksum and recovery SHALL take the other copies, as
`specification/storage/STORAGE.md` sections 17.2 and 19.2 state.

#### Scenario: A crash tears copy 2
- **WHEN** a crash during step 4 of a checkpoint tears copy 2 after copies 0 and 1 were written and synced
- **THEN** recovery keeps copies 0, 1 and 3, takes the newer sequence whose references verify, and opens

### Requirement: Failed syncs and write errors stop the node
A failed sync SHALL stop the node, retrying nothing, and nothing the sync covered SHALL be trusted; an I/O error
writing a tile SHALL stop the node, and one reading a tile SHALL lead to repair, as
`specification/storage/STORAGE.md` section 19.2 states.

#### Scenario: A failed grid sync
- **WHEN** the grid sync of step 3 of a checkpoint fails
- **THEN** the node stops without writing a superblock copy, and on restart recovery opens at the previous checkpoint
  and replays to the same checkpoint with the same bytes

#### Scenario: A failed tile write
- **WHEN** a tile write completes with an I/O error
- **THEN** the node stops, and the write is not retried

### Requirement: A refused punch changes nothing else
A punch the file system refuses SHALL leave the extent allocated on disk and free in the free set, and SHALL change
nothing else, as `specification/storage/STORAGE.md` sections 12.4 and 19.2 state; the extent's old bytes are never
read again.

#### Scenario: Two replicas, one refusing punches
- **WHEN** two replicas are fed the same rounds and only one's file system refuses every punch
- **THEN** both hold the same free sets, tiles, addresses and checkpoint identities, and their grids are byte-identical
  where freed extents are taken to read as zero

### Requirement: No answer from a tile that failed a check
After every injected fault the engine SHALL repair it, refuse with the refusal section 19.2 names, or stop, and SHALL
never answer from a tile that failed a check, as `specification/storage/STORAGE.md` sections 6.5 and 19.5 state.

#### Scenario: Every fault meets one of three responses
- **WHEN** a seeded run injects faults of every class
- **THEN** each fault is followed by its repair, its refusal or the node's stop, and the shadow model agrees with every
  answer given

### Requirement: The invariants are checked
The simulator SHALL check every invariant of `specification/storage/STORAGE.md` section 19.3 where it applies:
`DigestConserved` before any rewrite's output is written; `RelationDigests` after every round, the forward and reverse
sums of each (tenant, graph, predicate) equal; `RoundTrip` for every tile compaction writes; `HorizonMonotone` for
every logged `H`; `PlacementFound` at every placement; `PurgeAbsent` for every answer and every tile written;
`PoolBound` after every apply; `CheckpointIdentity` between replicas at every checkpoint position.

#### Scenario: A one-sided relation is caught
- **WHEN** a fault-free run is altered so a forward insert applies without its reverse insert
- **THEN** `RelationDigests` fails at that round's boundary

#### Scenario: A purged instance never reappears
- **WHEN** a run purges an instance and then reads at versions below and above the purge, before and after compaction
- **THEN** no answer and no tile written holds an entry naming the instance

### Requirement: A violated invariant is an invariant error
A violation of an invariant of `specification/storage/STORAGE.md` section 19.3 SHALL be an invariant error (Ultra
Style rule 5): it SHALL stop the replica in a release build and the caller in a debug build.

#### Scenario: A conservation failure stops the replica
- **WHEN** a release build's rewrite finds its outputs' digests not conserved
- **THEN** the replica stops with `DigestConserved` before placing any output

### Requirement: The shadow model answers every read
A shadow model, a map from every key to its entries with their versions, SHALL answer every read the engine answers,
at every version at or above the logged `H`, and the engine's answer SHALL equal it, as
`specification/storage/STORAGE.md` section 19.5 states.

#### Scenario: Reads across compaction and checkpoints
- **WHEN** a seeded run reads keys at versions from the logged `H` to the latest while rewrites and checkpoints happen
- **THEN** every answer equals the shadow model's

### Requirement: Replicas are byte-identical
Two engines fed the same rounds with different read loads, cache sizes and I/O completion orders SHALL hold the same
tiles at the same addresses and the same checkpoint identity at every checkpoint, and byte-identical grids where their
replica numbers are equal and freed extents read as zero, under every fault class of
`specification/storage/STORAGE.md` section 19.1, as sections 2 and 19.5 state. A difference SHALL be a defect, never a
tolerance.

#### Scenario: Different loads, one grid
- **WHEN** two replicas of replica number 1 apply one log, one under heavy reads with a small cache and reversed I/O
  completions, the other with no reads
- **THEN** at every checkpoint both grids hash to one XXH3-64 with freed extents read as zero, and their identities are
  equal

#### Scenario: Different replica numbers, one identity
- **WHEN** replicas 1 and 2 apply one log
- **THEN** their grids differ only in extent 0, and their checkpoint identities are equal at every position

### Requirement: A crashed replica replays to the same grid
A third engine that crashed and recovered SHALL, after replay, hold the same tiles at the same addresses, the same
checkpoint identities and, with an equal replica number, the same grid as the engines that did not crash, as
`specification/storage/STORAGE.md` sections 2, 17.2 and 19.5 state.

#### Scenario: A crash during every checkpoint step
- **WHEN** a replica is crashed in turn before and during each step 0 to 6 of one checkpoint, and each time recovered
  from its grid alone and replayed
- **THEN** its next checkpoint and its grid after the last round equal those of a replica that did not crash

### Requirement: GridFull is reached and left under faults
The simulator SHALL drive grids to `GridFull` under every fault class and check that a partition whose delta holds
changes, or which has released an extent since its last checkpoint, leaves it by its logged checkpoint round, and that
one whose every extent holds live data stays full and refuses every commit with code 5, `detail` 2, as
`specification/storage/STORAGE.md` sections 12.4 and 19.4 state; `PlacementFound` SHALL hold throughout.

#### Scenario: A full grid leaves by its flagged round
- **WHEN** a seeded run fills a grid below its reserve while the delta holds changes, and faults are injected meanwhile
- **THEN** the flagged round's checkpoint and the round after it bring `free` back to the reserve or above, every
  replica frees the same extents at the same boundary, and no placement finds no free extent

#### Scenario: A grid of live data stays full
- **WHEN** a run fills every extent with live data and empties the delta
- **THEN** every later commit is refused with code 5, `detail` 2, and no round is flagged

### Requirement: The reference engine's run is reproduced
An engine fed the 220 rounds of `vectors/engine.json` under its `settings` SHALL reproduce the reference engine of
`specification/conformance/storage/generate_vectors.py`, as `specification/storage/STORAGE.md` section 22 states: the
admission figures found before each round; every rewrite in completion order with its boundary, its deciding
boundary, its inputs' tile keys and its outputs' tile keys, addresses, lengths and checksums; every checkpoint's
figures and grid hash; and the final grid. A round flagged `checkpoint` SHALL be taken as a checkpoint position
whatever the grid holds.

#### Scenario: Admission, events and checkpoints
- **WHEN** the rounds of `vectors/engine.json` are applied
- **THEN** each round's delta, free extents and reserve equal its entry of `admission`, the rewrites equal `events` in
  order, and each checkpoint equals its entry of `checkpoints`, its grid's XXH3-64 included, with freed extents read
  as zero and replica number 1

#### Scenario: Flagged rounds far from full
- **WHEN** the empty rounds 25 and 150, flagged `checkpoint`, are applied while free extents stay at 55 or more against
  a reserve of at most 10
- **THEN** each is a checkpoint position; extent 4, which checkpoint 150's step 6 freed, is free at round 151's
  boundary before its placements; and round 151, crossing a byte threshold, is a checkpoint position too

#### Scenario: The final grid
- **WHEN** the last round, 220, has been applied
- **THEN** every nonzero block of the grid equals `tiles/engine_grid.blocks`, and the grid's XXH3-64 is
  `final.grid_xxh3_64`

### Requirement: The reference engine's crash is reproduced
A second replica of the reference run crashed after round 30 SHALL recover from its grid alone to the checkpoint at 25,
replay rounds 26 onward, and reach checkpoints and a final grid equal to the first replica's, as `vectors/engine.json`'s
`crash` states and `specification/storage/STORAGE.md` sections 17.2 and 22 require.

#### Scenario: Crash after round 30
- **WHEN** a replica applying the rounds of `vectors/engine.json` is crashed after round 30, reopened and replayed
- **THEN** it recovers to position 25, its later checkpoints equal `checkpoints`, and its final grid equals
  `tiles/engine_grid.blocks`
