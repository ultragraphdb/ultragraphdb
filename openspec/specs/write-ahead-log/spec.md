# write-ahead-log Specification

## Purpose
The write-ahead log: the ordered-log interface, the WAL's frames, segments and superblock, durability, recovery
and its corruption rules, checkpoints, the power-loss-protection flag, and its simulated storage.
## Requirements
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

### Requirement: A round is durable before it is applied
A round SHALL be written to the log and synced before any of it is applied to the store or acknowledged, with one sync
per round on any drive. A failed write or sync SHALL poison the log until the node restarts.

#### Scenario: A round never reaches the store before the log
- **WHEN** the node loses power at any step of a round
- **THEN** the store after recovery holds no write of a round the log does not hold

### Requirement: Rounds are frames in segments
A round SHALL be written as its frame, header and body, at a 4,096-byte boundary, padded to whole blocks, in one write,
into preallocated segment files of a fixed size; a round SHALL never span two segments. Its header SHALL carry its
position, its parent's checksum, the cluster and the log's incarnation, so a scan stops at the first block that is not
the next round of this incarnation.

#### Scenario: Preallocated zeros are not read as rounds
- **WHEN** the log is opened with a segment holding three rounds followed by zeros
- **THEN** the scan finds three rounds and its tail after the third

#### Scenario: Rounds of an older incarnation are not read
- **WHEN** a reused segment holds rounds of a previous incarnation after the current rounds
- **THEN** the scan stops after the current incarnation's rounds

### Requirement: Recovery replays the log above the store
On start, recovery SHALL read the position of the store's last checkpoint from its grid's superblock, the authority
for it (`specification/storage/STORAGE.md` sections 12.3 and 17.2), and apply every later round in order. Recovery SHALL
refuse to start, naming both positions, when the log's first round kept is above the position after the checkpoint's;
naming the position, when the checkpoint's position is above the log's tail; and when the log has a gap between the
checkpoint's position and its tail.

#### Scenario: Rounds above the checkpoint are replayed
- **WHEN** rounds 1 to 10 are synced in the log, the store's last checkpoint holds round 7, and power is lost
- **THEN** after recovery the store holds rounds 1 to 10

#### Scenario: A store ahead of the log refuses to start
- **WHEN** the store's checkpoint position lies beyond the log's tail
- **THEN** the node refuses to start, naming the position

#### Scenario: A store behind the log's trim refuses to start
- **WHEN** a checkpoint at position 40 has trimmed the log, and the store reopened holds a checkpoint at position 30
- **THEN** the node refuses to start, naming 30 and 40, and replays nothing

### Requirement: Corruption is detected and never read as absence
A scan SHALL bound a round's size before allocating for it, then check the header checksum, the parent chain, the
chunk table and each chunk, in that order. Damage SHALL be truncated only when no complete round follows it, and
every truncation SHALL be reported; damage with a complete round after it SHALL refuse the start, naming the round,
the chunk and the transactions the chunk holds. On one node, a round whose write the drive tore while reporting success
is lost with the truncation; the report names it.

#### Scenario: A torn tail is truncated
- **WHEN** the last round was torn by a crash during its write, before its sync completed, and no complete round
  follows it
- **THEN** the log is truncated before it, the truncation is reported, and the round was never acknowledged

#### Scenario: Interior corruption refuses to start
- **WHEN** a byte of an acknowledged round's body is flipped and later rounds follow it
- **THEN** the node refuses to start with an error naming the round, the chunk and its transactions

### Requirement: The superblock
The superblock SHALL be two copies written alternately, each through a temporary file synced, renamed over the older
copy, and its directory synced; it SHALL record its segments alone: the cluster and the incarnation their rounds carry,
the first segment kept, and the position and frame checksum of the last round its trim removed, which the first round
kept names as its parent. The checkpoint is the store's grid superblock's, which is authoritative
(`specification/storage/STORAGE.md` section 12.3). Reading it SHALL give one of three outcomes: empty, present, or unreadable; an unreadable
copy SHALL refuse the start and every write, and SHALL never be read as a fresh log.

#### Scenario: A corrupt copy is not a fresh deployment
- **WHEN** one superblock copy is corrupted
- **THEN** the node refuses to start with an error naming the copy, and does not create a new log

### Requirement: Checkpoints trim the log
A checkpoint SHALL follow `specification/storage/STORAGE.md` section 17.1: the store's grid synced and its superblock's
four copies written first, and only then the log's superblock written and the log trimmed through the checkpoint's
position, the segments holding no round above it zeroed, synced and kept as spares, which a new segment reuses before
one is created. A trim SHALL never remove or zero a round above the position of the grid's durable superblock, and a
crash between the grid's superblock and the trim SHALL leave the checkpoint in force, the trim done again at the next.

#### Scenario: A checkpoint recycles old segments
- **WHEN** rounds fill four segments and a checkpoint is taken at the end of the third
- **THEN** the first three segments are kept as spares holding only zeros, and recovery replays from the checkpoint

### Requirement: The power-loss-protection flag tunes and never weakens
A configuration flag SHALL state whether the log's drive has power-loss protection, `false` by default. It SHALL set the
defaults of the round bounds and the checkpoint interval only, and SHALL never remove or delay a sync.

#### Scenario: The flag does not remove the sync
- **WHEN** the flag is `true` and a round is committed
- **THEN** the round is synced before it is acknowledged

### Requirement: The log's storage is simulated
The log SHALL write through a storage interface with a compio implementation and a simulated one. The simulated storage
SHALL keep each file's and directory's buffered and stable contents, make a file sync stabilise its contents and a
directory sync its entries, return both to stable on a power loss, and inject faults before or after an operation, a
torn write reported as an error, and a torn write not reported. After any fault and recovery, every acknowledged round
SHALL be present, except a round whose write the storage tore while reporting success, which recovery SHALL detect
and report by name.

#### Scenario: A reported fault loses no acknowledged round
- **WHEN** the simulated storage fails a round's sync with an error, or loses power after a round was acknowledged
- **THEN** after recovery every acknowledged round is present

#### Scenario: A silent torn write is caught
- **WHEN** the simulated storage tears the last round's write without reporting it, and the node restarts
- **THEN** recovery detects the round by its checksums, truncates it as a torn tail and reports the truncation; it
  never reads the round as valid

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
against; and the derivation version it was applied under (`specification/storage/STORAGE.md` section 3.2). Replaying a
record SHALL derive the same state on every replica; a record naming another derivation version than the reader's own
SHALL be refused, naming both.

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

#### Scenario: Another derivation version is refused
- **WHEN** a record names a derivation version other than the reader's
- **THEN** replay fails with an error naming both versions, and applies nothing of the record

### Requirement: The log is the only synced stream
The store SHALL sync its grid at checkpoints alone (`specification/storage/STORAGE.md` section 17.1), and the log SHALL be
the only stream synced per round. After a crash the store SHALL open at its last durable checkpoint, and recovery SHALL
replay the log from the position its grid's superblock records.

#### Scenario: A crash loses no acknowledged round
- **WHEN** rounds are acknowledged, the process is killed between two checkpoints, and it restarts
- **THEN** every acknowledged round is held after recovery, replayed from the log above the last checkpoint's position

