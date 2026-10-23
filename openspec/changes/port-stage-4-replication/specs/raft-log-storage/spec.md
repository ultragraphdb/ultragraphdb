## ADDED Requirements

### Requirement: The log implements raft-rs storage
The Raft log SHALL implement raft-rs's `Storage`: initial state, entries in a range within a size limit, the term
of an index, the first and last index, and the snapshot, for one log per group, together with the node's
Raft ID and group ID, one log per group.

#### Scenario: Entries read back as written
- **WHEN** entries 1 to 1,000 are appended and entries 200 to 300 are read
- **THEN** the result is those entries with their terms, within the size limit given

#### Scenario: The node's identifiers survive a restart
- **WHEN** a node records its Raft ID and group ID and restarts
- **THEN** it reads back the same two values

### Requirement: Conflicting appends truncate the log
Appending an entry whose index the log already holds SHALL remove that entry and every later one before the append.

#### Scenario: A conflicting append replaces the suffix
- **WHEN** the log holds entries 1 to 10 in term 1 and entries 6 to 8 in term 2 are appended
- **THEN** the log holds 1 to 5 in term 1 and 6 to 8 in term 2, and nothing above 8

### Requirement: Snapshots compact the log
Recording a snapshot at index `i` SHALL keep its metadata and configuration and delete the entries at or below `i`,
so that the first index becomes `i + 1` and the term of `i` is still answered.

#### Scenario: A snapshot compacts the log
- **WHEN** a snapshot is recorded at index 500
- **THEN** the first index is 501, the snapshot reports index 500 and its term, and the term of index 500 is
  answered

### Requirement: Persistence and durability modes
The entries and hard state of one `Ready` SHALL be made durable together: after a crash the log SHALL hold all or none
of a `Ready`'s entries, and its hard state SHALL match its entries. In
`process` mode (the default, as in Dgraph) they SHALL survive a crash of the process, and the log and the store SHALL
be synced once a minute; in `filesystem` mode the log SHALL be synced, when the `Ready` must sync, before a
follower's messages leave, and the store SHALL be synced after each applied batch.

#### Scenario: A killed process loses nothing it acknowledged
- **WHEN** a replica is killed while appending and restarted
- **THEN** its log holds every entry it acknowledged and no torn entry, and its hard state matches its log

#### Scenario: Filesystem mode syncs before a follower replies
- **WHEN** a follower in `filesystem` mode acknowledges an append whose `Ready` must sync
- **THEN** the entries and hard state behind the reply were synced first

### Requirement: Entry payloads match Dgraph's encoding
An entry's data SHALL be the proposal key as a u64 big-endian followed by the proposal protobuf (`pb.Proposal` for
Alphas, `pb.ZeroProposal` for Zero), as Dgraph writes it, so a dump of either implementation's log can be compared
entry by entry.

#### Scenario: A log dump lists proposals
- **WHEN** the dump tool reads a group's log
- **THEN** it lists each entry's index, term, type, proposal key and decoded proposal

### Requirement: Records are framed and checksummed
Each log record SHALL carry its length and the CRC32C of its body, and recovery SHALL end the log at the first record
that is short or fails its checksum; the hard state, configuration, snapshot metadata and node identifiers SHALL sit
in two checksummed superblock copies written alternately.

#### Scenario: A torn tail is removed at recovery
- **WHEN** the last record of a segment is written half and the process crashes
- **THEN** after recovery the log ends at the record before it and holds every entry synced before it

#### Scenario: A torn write the device reported as complete is refused
- **WHEN** a record is written short while the write reports success, and the log is reopened
- **THEN** the record's checksum fails and the log ends before it

#### Scenario: A torn superblock write keeps the other copy
- **WHEN** a superblock write is torn
- **THEN** recovery reads the other copy and the hard state it holds

### Requirement: Log I/O runs through one storage trait
All log file access SHALL go through an asynchronous storage trait with two implementations: compio for production,
and a simulated one that injects a failure before or after an operation, torn writes reported and unreported, process
crashes and power loss.

#### Scenario: A power loss keeps every synced entry
- **WHEN** the log runs on the simulated storage and a power loss is injected after some appends were synced
- **THEN** after recovery the log holds every synced entry and no entry in part

