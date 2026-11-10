## MODIFIED Requirements

### Requirement: Recovery replays the log above the store
On start, recovery SHALL read the position of the store's last checkpoint from its grid's superblock, the authority
for it (`specification/storage/STORAGE.md` sections 12.3 and 17.2), and, from an empty delta (tile-grid, "Recovery
rebuilds from the superblock and replays"), apply every later round in order: each committed entry at its commit
version under the derivation its round names, each round's evictions before its entries, and its derivations as
logged (`specification/binary_protocol/PROTOCOL.md` section 12.2, *Recovery*). A build whose start the log holds
without its completion SHALL resume from its build record as the checkpoint holds it (`STORAGE.md` section 9.3, kind
15), and each undeclared predicate a replayed round first wrote SHALL be asked of the metadata group for its default
entry again. Recovery SHALL refuse to start, naming both positions, when the log's first round kept is above the
position after the checkpoint's; naming the position, when the checkpoint's position is above the log's tail; and when
the log has a gap between the checkpoint's position and its tail.

#### Scenario: Rounds above the checkpoint are replayed
- **WHEN** rounds 1 to 10 are synced in the log, the store's last checkpoint holds round 7, and power is lost
- **THEN** after recovery the store holds rounds 1 to 10

#### Scenario: A store ahead of the log refuses to start
- **WHEN** the store's checkpoint position lies beyond the log's tail
- **THEN** the node refuses to start, naming the position

#### Scenario: A store behind the log's trim refuses to start
- **WHEN** a checkpoint at position 40 has trimmed the log, and the store reopened holds a checkpoint at position 30
- **THEN** the node refuses to start, naming 30 and 40, and replays nothing

#### Scenario: A build interrupted by a crash resumes from the checkpoint's record
- **WHEN** a build's start round lies below the last checkpoint, its completion is not in the log, and the process is
  killed
- **THEN** recovery resumes the build strictly after the last key its record holds at that checkpoint, and does not
  begin it again from its first key

### Requirement: Checkpoints trim the log
A checkpoint SHALL be taken at the positions `specification/storage/STORAGE.md` section 17.1 makes due, a function of
the log alone: each round boundary at which the Round frames' total length first reaches the next multiple of
`checkpoint_bytes`, and each round flagged `checkpoint` (tile-grid, "Checkpoint positions"). It SHALL follow section
17.1's steps (tile-grid, "A checkpoint drains, publishes and frees"): the store's grid synced and its superblock's four
copies written first, the superblock recording in `log_bytes` the Round frames' total length through its position, and
only then the log's superblock written and the log trimmed through the checkpoint's position, the segments holding no
round above it zeroed, synced and kept as spares, which a new segment reuses before one is created. A trim SHALL never
remove or zero a round above the position of the grid's durable superblock, and a crash between the grid's superblock
and the trim SHALL leave the checkpoint in force, the trim done again at the next. A clean shutdown SHALL sync the log
and take no checkpoint.

#### Scenario: A checkpoint recycles old segments
- **WHEN** rounds fill four segments and a checkpoint is taken at the end of the third
- **THEN** the first three segments are kept as spares holding only zeros, and recovery replays from the checkpoint

#### Scenario: Replicas checkpoint at the same positions
- **WHEN** two replicas of one partition apply the same rounds under the same `checkpoint_bytes`
- **THEN** both take their checkpoints at the same positions and trim their logs through them

#### Scenario: A crash before the trim leaves the checkpoint in force
- **WHEN** the grid's four superblock copies are written and the process is killed before the log's superblock is
  written
- **THEN** recovery opens at the new checkpoint and replays the rounds above it, and the next checkpoint trims the log
  through its own position

#### Scenario: A clean shutdown takes no checkpoint
- **WHEN** a node shuts down cleanly between two checkpoint positions and restarts
- **THEN** its grid's superblock names the checkpoint before the shutdown, and recovery replays the rounds above it

### Requirement: A round's record is logical
A round's commits SHALL carry one epoch, since a round closes when a close is delivered (commit-rounds, "Catalog
changes, drops and index builds are ordered in rounds"). A round's record SHALL hold, for each committed transaction,
its resolved quads with their system IDs, its commit version, its drops as operations naming their target (a predicate,
a graph, a tenant's data or everything in a tenant), which replay applies as drop markers, its deletes by pattern as
their resolved removals, the session-table entry its commit writes, keyed by the session alone (the session ID, the
request number, the share number for an Ingest share and the stored reply), and the registrations it relied on as a
referrer, each the entity's ID, the bucket group and the generation the reply named; for each committed Ingest share,
its body as received and its resolution (ingest, "A share's round logs its body as received with its resolution"); the
new dictionary entries the round creates, a copied entry of another partition's entity in both directions counting as
one; each registration the partition accepted as an entity's home, with its own commit version and the referrer-set
member it adds or renews (term-dictionary, "A reference to another partition's entity registers at its home"); an index
build's start and completion records (commit-rounds); the derivations execution made that apply cannot, each count
row's degree before the round and each `@limit` aggregate's and statistics change the round makes, which apply writes as
additive records (T48); the sessions evicted at the round, the first round after the epoch close evicting them is
delivered, each applied as a record delete; the Round command header's `flags`, whose bit 0, `checkpoint`, a primary
sets while `GridFull` holds its partition and every replica takes as a checkpoint position
(`specification/storage/STORAGE.md` section 17.1); the fold horizon its primary stamped (`STORAGE.md` section 14.2); the
one catalog version the round was validated against; and the derivation version it was applied under
(`specification/storage/STORAGE.md` section 3.2). Replaying a record SHALL derive the same state on every replica; a
record naming another derivation version than the reader's own SHALL be refused, naming both.

#### Scenario: Replay derives the same state
- **WHEN** a round holding a first reference to an entity another partition owns, with its registration mark and the
  copied ID-to-IRI entry, is applied, the store is discarded, and the round's record is replayed into an empty store
- **THEN** the store answers every read exactly as it did after the first apply, the mark's generation and the copied
  entry included

#### Scenario: Replay rebuilds drops and the session table
- **WHEN** a store holds `p`'s statements and statements a delete by pattern will remove, a round holding a drop of `p`
  and the next round holding that delete by pattern and a commit of session `s`'s request 7 are applied, the store is
  restored to its state before them, and their records are replayed onto it
- **THEN** no read answers an entry of `p` or any of the removed statements, and a retry of `s`'s request 7 returns its
  stored reply without committing again

#### Scenario: Replay evicts sessions at the round that logs them
- **WHEN** an epoch close evicting session `s` is delivered, the next round logs `s` among its evictions, and the store
  is restored to its state before that round and the round is replayed onto it
- **THEN** the session table holds no entry for `s` after the replay, as it held none after the first apply, and a
  follower that applied the round before the close reached it holds the same table, since the round evicts, not the
  close's arrival

#### Scenario: A round flagged checkpoint is a checkpoint position
- **WHEN** a primary whose partition `GridFull` holds logs a round with `flags` bit 0 set, below the next multiple of
  `checkpoint_bytes`
- **THEN** every replica applying the round checkpoints at its position, and replay of the same rounds checkpoints there
  too

#### Scenario: Replay derives a home's referrer-set deltas
- **WHEN** a round holding only two registrations a home accepted is applied, the store is discarded, and the record is
  replayed
- **THEN** the referrer set holds both members, each at the commit version the record lists for its registration

#### Scenario: Another derivation version is refused
- **WHEN** a record names a derivation version other than the reader's
- **THEN** replay fails with an error naming both versions, and applies nothing of the record
