## MODIFIED Requirements

### Requirement: The store is reached through one trait
Everything above the store SHALL reach a partition's engine through its logical interface of rounds to apply, lane
requests and their answers, and the engine and the grid SHALL be state machines with no I/O of their own, whose I/O
requests a shell performs (`specification/storage/STORAGE.md` section 2). The log and the grid SHALL perform every file
operation, a read, a vectored write, a sync, an extent's allocation and a hole's punch, through one durable-storage
interface with two implementations: compio's for production and a simulated one. Outside those two implementations no
storage code SHALL read the wall clock, spawn a thread or draw from an ambient random source.

#### Scenario: The log and the grid run on the simulated storage
- **WHEN** one workload's rounds are logged and applied once on the compio storage and once on the simulated storage
  with no fault injected
- **THEN** both runs pass the same checks, and their grids hold the same bytes

### Requirement: A simulation run is a function of its seed
The simulator SHALL derive the workload, the faults, the order in which it steps the apply loop and its readers, and
the order in which I/O completes from one seed, through separate streams, and SHALL advance time only itself; a failing
run SHALL print its seed, and running that seed again SHALL reproduce the run step for step.

#### Scenario: A seed replays its run
- **WHEN** a seed is run twice
- **THEN** both runs perform the same operations, inject the same faults, complete I/O in the same order and end in the
  same state

### Requirement: The simulator injects storage faults
The simulated storage SHALL keep each file's and directory's buffered and stable contents, make a file sync stabilise
the file's contents and a directory sync its entries, and inject, as the run's fault stream draws them, every fault of
`STORAGE.md` section 19.1 against the log's and the grid's files alike: a crash at any operation, which loses what was
not stable; a power loss at any moment, likewise; a write torn at any 512-byte boundary, reported as an error or not;
a write the storage acknowledged and lost; a write or a read misdirected to another block; a latent sector error on a
read; bits flipped at rest; a failed sync, after which nothing the sync covered is trusted; and a punch the file system
refuses. After a failed write or sync, the only next step SHALL be the node's stop and recovery; after any fault and
recovery every acknowledged round SHALL be present, except a round whose write the storage tore while reporting
success, which recovery detects and reports by name (write-ahead-log, "The log's storage is simulated"), and the grid
SHALL answer as tile-simulation's "No answer from a tile that failed a check" states.

#### Scenario: A power loss keeps every acknowledged round
- **WHEN** rounds are acknowledged, tiles written since the last checkpoint, and a power loss is injected
- **THEN** after recovery from the grid's last checkpoint and the log, every acknowledged round is applied, and no tile
  written after that checkpoint is read before replay writes it again

#### Scenario: A failed sync trusts nothing it covered
- **WHEN** the grid's sync at a checkpoint fails
- **THEN** the node stops, no superblock copy of that checkpoint is written, and recovery opens at the previous
  checkpoint

#### Scenario: A refused punch leaves the bytes and frees the extent
- **WHEN** the simulated storage refuses a checkpoint's punch of a freed extent
- **THEN** the extent's bytes stay on disk, the free set marks it free, and nothing else changes

#### Scenario: A misdirected read is not answered
- **WHEN** a read of a tile is misdirected to another block
- **THEN** the bytes read are refused by the tile's checks, no answer uses them, and the tile is repaired or the node
  stops

### Requirement: Invariants hold after every step
After every step and every recovery the simulator SHALL check the store against a shadow model: every acknowledged
round present, a read at every version at or above the logged fold horizon answering what the shadow model answers, a
read repeated through an open snapshot returning what it returned the first time, a dropped or purged entry absent at
every version, and every invariant of `STORAGE.md` section 19.3 (tile-simulation, "The invariants are checked"). At
every checkpoint it SHALL check that replicas fed the same rounds under different read loads, cache sizes, I/O
completion orders and faults hold the same tiles at the same addresses and the same checkpoint identity
(tile-simulation, "Replicas are byte-identical").

#### Scenario: A violated invariant names its seed
- **WHEN** a run breaks an invariant
- **THEN** the simulator stops and reports the invariant, the step and the seed

#### Scenario: The dictionary workload passes on the simulator
- **WHEN** a seeded workload creates terms and writes quads naming them under every fault class
- **THEN** every stored system ID and literal hash has its dictionary entry after every step and every recovery

#### Scenario: Two replicas agree under faults
- **WHEN** two replicas apply one log, one of them under torn, lost and misdirected writes and a power loss
- **THEN** at every checkpoint both hold the same tiles at the same addresses and the same checkpoint identity

## REMOVED Requirements

### Requirement: The simulated store models the keyspace without versions
**Reason**: The tile store has no keyspaces and no versionless keyspace: every structure is a tile in the grid or an
entry in the delta, and a round's changes become visible together at its apply (`specification/storage/STORAGE.md`
sections 14 and 15).
**Migration**: A round visible whole is the shadow model's check in "Invariants hold after every step" and
tile-simulation's "The shadow model answers every read"; the dictionary workload's scenario moves to "Invariants hold
after every step".
