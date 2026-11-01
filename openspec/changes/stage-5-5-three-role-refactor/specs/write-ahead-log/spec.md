## MODIFIED Requirements

### Requirement: The ordered-log interface
The log SHALL be reached through one interface whose local implementation is the write-ahead log and whose stage-8
implementation is VSR: `append` accepts a round only at the next position and with the previous round's checksum as
its parent, is refused otherwise, and returns a durability ticket that completes once the round is durable;
`committed` returns the highest durable position and never decreases; `read` returns rounds in order, each checksum and
parent verified; `trim_through` removes rounds a durable checkpoint covers; `seal` makes every later append fail and
returns the final position.

#### Scenario: An append out of order is refused
- **WHEN** the log holds rounds 1 to 5 and round 7 is appended
- **THEN** the append is refused with an error naming position 6, and nothing is written

#### Scenario: A sealed log refuses appends
- **WHEN** the log is sealed at position 9 and a round is appended
- **THEN** the append fails, and `seal` reported 9

#### Scenario: A ticket completes once its round is durable
- **WHEN** a round is appended and its ticket is polled before and after the sync that covers it
- **THEN** the ticket is pending before the sync and complete after it, and `committed` reaches the round's position
  only then

### Requirement: Checkpoints trim the log
At a checkpoint the store SHALL be explicitly synced, since its journal is never synced on its own, the checkpoint's
position recorded in the superblock, and the segments wholly below it zeroed, synced and kept as spares, which a new
segment reuses before one is created; a trim SHALL never remove or zero a round the store has not durably applied.

#### Scenario: A checkpoint recycles old segments
- **WHEN** rounds fill four segments and a checkpoint is taken at the end of the third
- **THEN** the first three segments are kept as spares holding only zeros, and recovery replays from the checkpoint

## ADDED Requirements

### Requirement: A round's record is logical
A round's record SHALL hold, for each committed transaction, its resolved quads with their system IDs, the new
dictionary entries the round creates, its commit version, the catalog version it was validated against and the version
of the derivation that turns quads into keys. Replaying a record SHALL derive the same keys and values on every replica
and in every version that reads it; a record naming a derivation version the reader does not know SHALL be refused,
naming the version.

#### Scenario: Replay derives the same keys
- **WHEN** a round is applied, the store is discarded, and the round's record is replayed into an empty store
- **THEN** the store holds exactly the keys and values it held after the first apply

#### Scenario: An unknown derivation version is refused
- **WHEN** a record names a derivation version above the reader's
- **THEN** replay fails with an error naming the version, and applies nothing of the record

### Requirement: The log is the only synced stream
The store SHALL write with its own journal unsynced (fjall's manual journal persistence), and the log SHALL be the only
stream synced per round. After a crash the store SHALL recover a prefix of its batches, and recovery SHALL replay the
log from the store's applied position, which every store batch records with its data.

#### Scenario: A crash loses no acknowledged round
- **WHEN** rounds are acknowledged, the process is killed before the store's journal is synced, and it restarts
- **THEN** every acknowledged round is held after recovery, replayed from the log above the store's applied position
