## ADDED Requirements

### Requirement: The ordered-log interface
The log SHALL be reached through one interface whose local implementation is the write-ahead log and whose stage-10
implementation is VSR: `append` accepts a round only at the next position and with the previous round's checksum as
its parent, and is refused otherwise; `committed` returns the highest durable position and never decreases; `read`
returns rounds in order, each checksum and parent verified; `trim_through` removes rounds a durable checkpoint covers;
`seal` makes every later append fail and returns the final position.

#### Scenario: An append out of order is refused
- **WHEN** the log holds rounds 1 to 5 and round 7 is appended
- **THEN** the append is refused with an error naming position 6, and nothing is written

#### Scenario: A sealed log refuses appends
- **WHEN** the log is sealed at position 9 and a round is appended
- **THEN** the append fails, and `seal` reported 9

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
On start, recovery SHALL read the store's applied version, find the round holding it, and apply every later round in
order; applying a round SHALL be idempotent. Recovery SHALL refuse to start when the store holds a version the log does
not, or when the log has a gap between the store's version and its tail.

#### Scenario: Rounds lost from the store are replayed
- **WHEN** rounds 1 to 10 are synced in the log, the store had applied up to round 7, and power is lost
- **THEN** after recovery the store holds rounds 1 to 10

#### Scenario: A store ahead of the log refuses to start
- **WHEN** the store's applied version belongs to no round in the log
- **THEN** the node refuses to start, naming the version

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
copy, and its directory synced; it SHALL hold the cluster, the incarnation, the last checkpoint's position and checksum,
and the first segment kept. Reading it SHALL give one of three outcomes: empty, present, or unreadable; an unreadable
copy SHALL refuse the start and every write, and SHALL never be read as a fresh log.

#### Scenario: A corrupt copy is not a fresh deployment
- **WHEN** one superblock copy is corrupted
- **THEN** the node refuses to start with an error naming the copy, and does not create a new log

### Requirement: Checkpoints trim the log
At a checkpoint the store SHALL be synced, the checkpoint's position recorded in the superblock, and the segments
wholly below it removed; a trim SHALL never remove a round the store has not durably applied.

#### Scenario: A checkpoint removes old segments
- **WHEN** rounds fill four segments and a checkpoint is taken at the end of the third
- **THEN** the first three segments are removed, and recovery replays from the checkpoint

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
