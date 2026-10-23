## ADDED Requirements

### Requirement: Versions are part of the key
The store SHALL keep every version of a key, storing a version as the user key with every `0x00` byte written as
`0x00 0xFF`, then the terminator `0x00 0x01`, then the bitwise complement of the version in big-endian order, so that
fjall's byte order is Badger's: keys in order, each key's versions contiguous and newest first. Each version SHALL carry
a meta byte (schema `0x01`, delta `0x04`, complete `0x08`, empty `0x10`), a discard flag and a tombstone flag. A write
whose user key exceeds 65,000 bytes, as Badger refuses it, or whose stored key exceeds 65,535 bytes, fjall's limit,
SHALL be refused with Badger's text `Key with size <n> exceeded <limit> limit`, and its batch SHALL write nothing.

#### Scenario: A key's versions iterate newest first
- **WHEN** a key is written at versions 3, 7 and 5
- **THEN** iterating its versions yields 7, 5 and 3, each with its meta byte

#### Scenario: A version below the newest can be written
- **WHEN** a key holds version 9 and a write at version 6 follows
- **THEN** both versions are kept, and the key's versions iterate as 9 then 6

#### Scenario: Rewriting a version replaces it
- **WHEN** the same key and version are written twice with different values
- **THEN** a read at that version returns the second value

#### Scenario: A key that prefixes another sorts first
- **WHEN** index keys for the terms `ab` and `abc` are each written at versions 3 and 5
- **THEN** a scan yields `ab` before `abc`, each key's versions contiguous and newest first

#### Scenario: A user key over Badger's limit is refused
- **WHEN** a batch writes a key whose user key is 65,001 bytes long
- **THEN** the batch fails with an error beginning `Key with size 65001 exceeded 65000 limit`, and none of its keys is
  found

#### Scenario: A stored key over fjall's limit is refused
- **WHEN** a batch writes a key whose user key is 40,000 zero bytes, 80,010 bytes once escaped and versioned
- **THEN** the batch fails with an error beginning `Key with size 80010 exceeded 65535 limit`, and none of its keys is
  found

### Requirement: Reads at a timestamp
A read of a key at timestamp `t` SHALL return the newest version at or below `t`, SHALL treat a deletion as absent,
and SHALL be refused with `Transaction is too old` when `t` is below the discard timestamp.

#### Scenario: A read takes the newest version at or below its timestamp
- **WHEN** a key holds versions 3 and 7 and is read at 5
- **THEN** the read returns version 3

#### Scenario: A deletion hides the key
- **WHEN** a key holds a value at 3 and a deletion at 6
- **THEN** a read at 7 finds nothing and a read at 4 finds the value

#### Scenario: A read below the discard timestamp is refused
- **WHEN** the discard timestamp is 10 and a key is read at 5
- **THEN** the read fails with `Transaction is too old`

### Requirement: Snapshot scans
Every read, point or scan, SHALL read one store snapshot covering the data and metadata keyspaces, and a read of many
keys SHALL use one snapshot. A prefix or range scan SHALL support seeking, forward and reverse order, and SHALL
report each key once with its newest version at or below the scan's timestamp, or, when asked, with each of its
versions newest first down to that timestamp.

#### Scenario: A scan is unaffected by a later batch
- **WHEN** a scan is open and a batch writes keys inside its range
- **THEN** the scan returns none of the batch's keys

#### Scenario: A scan reports each key once
- **WHEN** three keys under a prefix hold several versions each and the prefix is scanned at a timestamp
- **THEN** each key appears once, with its newest version at or below that timestamp

#### Scenario: A scan yields every version when asked
- **WHEN** the same prefix is scanned for all versions
- **THEN** each key's versions at or below the timestamp appear newest first

### Requirement: Atomic write batches
A write batch SHALL become visible whole or not at all, to point reads and scans alike, MAY hold entries at several
versions, SHALL have no size limit that splits it, and SHALL be written with at least `PersistMode::Buffer`. Until a
Raft log carries durability, the store SHALL offer Dgraph's two modes: `process`, which hands each commit batch to the
operating system and syncs once a minute, and `filesystem`, which syncs a commit batch before it is acknowledged. A
write SHALL count as durable once a sync has covered it.

#### Scenario: A reader sees a batch whole or not at all
- **WHEN** a batch writing 10,000 keys commits while readers read those keys repeatedly
- **THEN** every reader sees either none or all of the batch's keys

#### Scenario: A crash during a batch leaves nothing of it
- **WHEN** the process is killed while a batch is written and the store is reopened
- **THEN** either the whole batch or none of it is present

#### Scenario: A batch holds entries at several versions
- **WHEN** one batch writes key `a` at version 12 and key `b` at version 13
- **THEN** a read at 12 finds `a` and not `b`, and a read at 13 finds both

### Requirement: Prefix drops and banned namespaces
Dropping prefixes SHALL remove every stored version of every key under them, physically. Commits to the prefixes SHALL
be blocked while the drop runs; the drop SHALL write a drop record (its prefixes, its timestamp and the entries to write
once the prefixes are empty) to the metadata keyspace and sync it, remove the versions with lsm-tree removals in as many
batches as needed, write the record's entries, sync, and delete the record; at open, a remaining record SHALL make the
store finish the drop before it serves. A read below the drop's timestamp SHALL find nothing of the dropped data.
Dropping everything SHALL follow the same pattern around clearing the data keyspace, SHALL leave only the entries it is
given, and SHALL clear the banned set, the discard timestamp, the tombstone horizon and the applied timestamp. A batch
SHALL drop the writes addressed to a banned namespace and write the rest, and a banned namespace SHALL be absent from
reads and scans; Badger refuses such writes with `ErrBannedKey`, and Dgraph ignores the error and loses the rest of the
Badger transaction.

#### Scenario: A dropped prefix is gone after reopening
- **WHEN** a prefix is dropped and the store is closed and reopened
- **THEN** no key under the prefix is found

#### Scenario: A drop interrupted by a crash finishes at open
- **WHEN** a prefix drop has synced its record and written part of its removals, and the process is killed
- **THEN** on reopening, the store finishes the drop before it serves a read, and no key under the prefix is found

#### Scenario: A read below a drop finds nothing of it
- **WHEN** a key under a prefix holds a version at 3, the prefix is dropped at 8, and the key is read at 5
- **THEN** the read finds nothing

#### Scenario: Dropping everything keeps only its entries
- **WHEN** everything is dropped with one entry to write at timestamp 1, and the store is closed and reopened
- **THEN** that entry is the only key found, and the applied timestamp and the discard timestamp are 0

#### Scenario: A banned namespace ignores writes
- **WHEN** namespace 5 is banned and a batch writes a key in it and a key in namespace 6
- **THEN** the batch succeeds, the key in namespace 5 is not found and the key in namespace 6 is

#### Scenario: Dropping everything clears the banned set
- **WHEN** namespace 5 is banned, everything is dropped, and a batch then writes a key in namespace 5
- **THEN** the key is found

### Requirement: Garbage collection below the discard timestamp
Given a discard timestamp `D` and a tombstone horizon `H` at or below it, compaction SHALL drop every version of a key
older than its newest discard-flagged or deletion version at or below `D`, SHALL never drop a deletion, and SHALL keep
every version above `D`. A sweep at one snapshot SHALL remove a deletion at `T` at or below `H` when no version of its
key remains below `T`, and no write SHALL land below a deletion of the same key. `H` SHALL equal `D` unless the store's
owner sets it lower. The compaction filter SHALL keep any item it cannot decode, and until the store has loaded `D` the
filter SHALL drop nothing. `D` SHALL never exceed the oldest open reader's timestamp.

#### Scenario: Versions below a discard point are collected
- **WHEN** a key holds deltas at 2 and 4, a complete version with the discard flag at 5 and a delta at 8, the discard
  timestamp is 6, and the memtables are flushed and a full compaction runs
- **THEN** versions 2 and 4 are gone, and versions 5 and 8 remain

#### Scenario: A deletion outlives an older version in another table
- **WHEN** a key's value at 2 and its deletion at 4 lie in two different last-level tables, the discard timestamp is 6,
  and a compaction merges the table holding the deletion without the other
- **THEN** the deletion remains, and a read at 7 finds nothing

#### Scenario: The sweep removes a deletion with nothing below it
- **WHEN** a key holds a value at 2 and a deletion at 4, the discard timestamp and the tombstone horizon are 6, and a
  full compaction and the sweep run
- **THEN** no version of the key remains

#### Scenario: A deletion above the tombstone horizon is kept
- **WHEN** a key holds a value at 2 and a deletion at 5, the discard timestamp is 6 and the tombstone horizon is 4, and
  a full compaction and the sweep run
- **THEN** the value at 2 is gone and the deletion at 5 remains

#### Scenario: An undecodable item survives compaction
- **WHEN** a stored value too short to hold the version prefix lies in a compaction
- **THEN** the compaction completes, the store is not poisoned, and the item remains

#### Scenario: An open reader holds the discard timestamp back
- **WHEN** a reader at timestamp 3 is open and a discard timestamp of 6 is requested
- **THEN** the discard timestamp used is at most 3 until the reader closes
