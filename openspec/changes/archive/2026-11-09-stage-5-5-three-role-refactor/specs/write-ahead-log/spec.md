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

### Requirement: Recovery replays the log above the store
On start, recovery SHALL read the store's applied log position, which every store batch records with its data, and
apply every later round in order; applying a round SHALL be idempotent. A checkpoint SHALL record its position.
Recovery SHALL refuse to start, naming both positions, when the store's applied position is below the checkpoint's;
naming the position, when the store's applied position is above the log's tail; and when the log has a gap between the
store's position and its tail.

#### Scenario: Rounds lost from the store are replayed
- **WHEN** rounds 1 to 10 are synced in the log, the store had applied up to round 7, and power is lost
- **THEN** after recovery the store holds rounds 1 to 10

#### Scenario: A store ahead of the log refuses to start
- **WHEN** the store's applied position lies beyond the log's tail
- **THEN** the node refuses to start, naming the position

#### Scenario: A store behind its checkpoint refuses to start
- **WHEN** a checkpoint at position 40 has trimmed the log, and the store reopened has applied position 30
- **THEN** the node refuses to start, naming 30 and 40, and replays nothing

## ADDED Requirements

### Requirement: A round's record is logical
A round's commits SHALL carry one epoch, since a round closes when a close is delivered (commit-rounds, "Catalog
changes, drops and index builds are ordered in rounds"). A round's record SHALL hold, for each committed transaction,
its resolved quads with their system IDs, its commit version, its drops as operations naming their target (a predicate,
a graph, a tenant's data or everything in a tenant), whose key removals replay derives, its deletes by pattern as their
resolved removals, the session-table entry its commit writes (the session ID, the request number and the stored reply),
and the registrations it relied on as a referrer, each the entity's ID, the bucket group and the generation the reply
named; the new dictionary entries the round creates, a copied entry of another partition's entity in both directions
counting as one; each registration the partition accepted as an entity's home, with its own commit version and the
referrer-set member it adds or renews (term-dictionary, "A reference to another partition's entity registers at its
home"); an index build's start and completion records (commit-rounds); the one catalog version the round was validated
against; and the version of the derivation that turns quads into keys. Replaying a record SHALL derive the same keys and
values on every replica and in every version that reads it; a record naming a derivation version the reader does not
know SHALL be refused, naming the version.

#### Scenario: Replay derives the same keys
- **WHEN** a round holding a first reference to an entity another partition owns, with its registration mark and the
  copied ID-to-IRI entry, is applied, the store is discarded, and the round's record is replayed into an empty store
- **THEN** the store holds exactly the keys and values it held after the first apply, the mark's generation and the
  copied entry included

#### Scenario: Replay rebuilds drops and the session table
- **WHEN** a store holds `p`'s keys and statements a delete by pattern will remove, a round holding a drop of `p` and
  the next round holding that delete by pattern and a commit of session `s`'s request 7 are applied, the store is
  restored to its state before them, and their records are replayed onto it
- **THEN** no key of `p` and none of the removed statements is held, and a retry of `s`'s request 7 returns its stored
  reply without committing again

#### Scenario: Replay derives a home's referrer-set deltas
- **WHEN** a round holding only two registrations a home accepted is applied, the store is discarded, and the record is
  replayed
- **THEN** the referrer set holds both members, each at the commit version the record lists for its registration

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
