## ADDED Requirements

### Requirement: Versions are part of the key
The store SHALL keep every version of a key, encoding a version as the key followed by the bitwise complement of
the version in big-endian order, so that a key's versions sort newest first. Each version SHALL carry a meta byte
(schema `0x01`, delta `0x04`, complete `0x08`, empty `0x10`), a discard flag and a tombstone flag.

#### Scenario: A key's versions iterate newest first
- **WHEN** a key is written at versions 3, 7 and 5
- **THEN** iterating its versions yields 7, 5 and 3, each with its meta byte

#### Scenario: A version below the newest can be written
- **WHEN** a key holds version 9 and a write at version 6 follows
- **THEN** both versions are kept, and the key's versions iterate as 9 then 6

#### Scenario: Rewriting a version replaces it
- **WHEN** the same key and version are written twice with different values
- **THEN** a read at that version returns the second value

### Requirement: Reads at a timestamp
A read of a key at timestamp `t` SHALL return the newest version at or below `t`, and SHALL treat a tombstone as
absent.

#### Scenario: A read takes the newest version at or below its timestamp
- **WHEN** a key holds versions 3 and 7 and is read at 5
- **THEN** the read returns version 3

#### Scenario: A tombstone hides the key
- **WHEN** a key holds a value at 3 and a tombstone at 6
- **THEN** a read at 7 finds nothing and a read at 4 finds the value

### Requirement: Snapshot scans
A prefix or range scan SHALL read one store snapshot, SHALL support seeking, forward and reverse order, and
SHALL report each key once with its newest version at or below the scan's timestamp.

#### Scenario: A scan is unaffected by a later batch
- **WHEN** a scan is open and a batch writes keys inside its range
- **THEN** the scan returns none of the batch's keys

#### Scenario: A scan reports each key once
- **WHEN** three keys under a prefix hold several versions each and the prefix is scanned at a timestamp
- **THEN** each key appears once, with its newest version at or below that timestamp

### Requirement: Atomic write batches
A write batch SHALL become visible whole or not at all, to point reads and scans alike, and SHALL have no size
limit that splits it.

#### Scenario: A reader sees a batch whole or not at all
- **WHEN** a batch writing 10,000 keys commits while readers read those keys repeatedly
- **THEN** every reader sees either none or all of the batch's keys

#### Scenario: A crash during a batch leaves nothing of it
- **WHEN** the process is killed while a batch is written and the store is reopened
- **THEN** either the whole batch or none of it is present

### Requirement: Prefix drops and banned namespaces
Dropping a prefix SHALL remove every key under it durably; dropping everything SHALL empty the store; a banned
namespace SHALL refuse writes silently and SHALL be absent from reads and scans.

#### Scenario: A dropped prefix is gone after reopening
- **WHEN** a prefix is dropped and the store is closed and reopened
- **THEN** no key under the prefix is found

#### Scenario: A banned namespace ignores writes
- **WHEN** namespace 5 is banned and a batch writes a key in it
- **THEN** the batch succeeds and the key is not found

### Requirement: Garbage collection below the discard timestamp
Given a discard timestamp `D`, compaction SHALL drop every version of a key older than its newest
discard-flagged version at or below `D`, and SHALL keep every version above `D`. `D` SHALL never exceed the
oldest open reader's timestamp.

#### Scenario: Versions below a discard point are collected
- **WHEN** a key holds deltas at 2 and 4, a complete version with the discard flag at 5 and a delta at 8, the
  discard timestamp is 6, and compaction runs
- **THEN** versions 2 and 4 are gone, and versions 5 and 8 remain

#### Scenario: An open reader holds the discard timestamp back
- **WHEN** a reader at timestamp 3 is open and a discard timestamp of 6 is requested
- **THEN** the discard timestamp used is at most 3 until the reader closes
