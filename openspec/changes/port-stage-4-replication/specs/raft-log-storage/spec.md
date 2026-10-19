## ADDED Requirements

### Requirement: The log implements raft-rs storage
The Raft log SHALL implement raft-rs's `Storage`: initial state, entries in a range within a size limit, the term
of an index, the first and last index, and the snapshot, for one group per keyspace, together with the node's
Raft ID, group ID and checkpoint index.

#### Scenario: Entries read back as written
- **WHEN** entries 1 to 1,000 are appended and entries 200 to 300 are read
- **THEN** the result is those entries with their terms, within the size limit given

#### Scenario: The node's identifiers survive a restart
- **WHEN** a node records its Raft ID, group ID and checkpoint index and restarts
- **THEN** it reads back the same three values

### Requirement: Conflicting appends truncate the log
Appending an entry whose index is already held SHALL remove that entry and every later one before the append.

#### Scenario: A conflicting append replaces the suffix
- **WHEN** the log holds entries 1 to 10 in term 1 and entries 6 to 8 in term 2 are appended
- **THEN** the log holds 1 to 5 in term 1 and 6 to 8 in term 2, and nothing above 8

### Requirement: Snapshots compact the log
Recording a snapshot at index `i` SHALL keep its metadata and configuration and delete the entries at or below `i`,
so that the first index becomes `i + 1`.

#### Scenario: A snapshot compacts the log
- **WHEN** a snapshot is recorded at index 500
- **THEN** the first index is 501 and the snapshot reports index 500 and its term

### Requirement: Persistence and durability modes
Hard state, entries and snapshot metadata of one `Ready` SHALL be persisted together, entries first. In `process`
mode (the default, as in Dgraph) they SHALL survive a crash of the process; in `filesystem` mode they SHALL be synced
before any message depending on them is sent.

#### Scenario: A killed process loses nothing it acknowledged
- **WHEN** a replica is killed while appending and restarted
- **THEN** its log holds every entry it acknowledged and no torn entry, and its hard state matches its log

#### Scenario: Filesystem mode syncs before replying
- **WHEN** a replica in `filesystem` mode votes or acknowledges an append
- **THEN** the hard state and entries behind the reply were synced first

### Requirement: Entry payloads match Dgraph's encoding
An entry's data SHALL be the proposal key as a u64 big-endian followed by the proposal protobuf (`pb.Proposal` for
Alphas, `pb.ZeroProposal` for Zero), as Dgraph writes it, so a dump of either implementation's log can be compared
entry by entry.

#### Scenario: A log dump lists proposals
- **WHEN** the dump tool reads a group's log
- **THEN** it lists each entry's index, term, type, proposal key and decoded proposal
