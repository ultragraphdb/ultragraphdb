## RENAMED Requirements

- FROM: `### Requirement: Prefix drops and banned namespaces`
- TO: `### Requirement: Prefix drops and banned tenants`

## MODIFIED Requirements

### Requirement: Prefix drops and banned tenants
Dropping prefixes SHALL remove every stored version of every key under them, physically. Commits to the prefixes SHALL
be blocked while the drop runs; the drop SHALL write a drop record (its prefixes, its timestamp and the entries to write
once the prefixes are empty) to the metadata keyspace and sync it, remove the versions with lsm-tree removals in as many
batches as needed, write the record's entries, sync, and delete the record; at open, a remaining record SHALL make the
store finish the drop before it serves. A read below the drop's timestamp SHALL find nothing of the dropped data.
Dropping everything SHALL follow the same pattern around clearing the data keyspace, SHALL leave only the entries it is
given, and SHALL clear the banned set, the discard timestamp, the tombstone horizon and the applied timestamp. A batch
SHALL drop the writes addressed to a banned tenant and write the rest, and a banned tenant SHALL be absent from
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

#### Scenario: A banned tenant ignores writes
- **WHEN** tenant 5 is banned and a batch writes a key in it and a key in tenant 6
- **THEN** the batch succeeds, the key in tenant 5 is not found and the key in tenant 6 is

#### Scenario: Dropping everything clears the banned set
- **WHEN** tenant 5 is banned, everything is dropped, and a batch then writes a key in tenant 5
- **THEN** the key is found

### Requirement: Garbage collection below the discard timestamp
Given a discard timestamp `D` and a tombstone horizon `H` at or below it, compaction SHALL drop every version of a key
older than its newest discard-flagged or deletion version at or below `D`, SHALL never drop a deletion, and SHALL keep
every version above `D`. A sweep at one snapshot SHALL remove a deletion at `T` at or below `H` when no version of its
key remains below `T`, and no write SHALL land below a deletion of the same key. `H` SHALL equal `D` unless the store's
owner sets it lower. The compaction filter SHALL keep any item it cannot decode, and until the store has loaded `D` the
filter SHALL drop nothing. `D` SHALL never exceed the oldest open reader's timestamp. A version a prefix drop writes
(the tenant's initial catalog) SHALL carry the fenced flag and SHALL never be collected. A version the
filter collects SHALL stay collected across a restart.

#### Scenario: The initial catalog a drop writes survives garbage collection
- **WHEN** everything in a tenant is dropped, its initial catalog is written at timestamp 1, the discard timestamp is
  raised above the drop and the store compacts
- **THEN** the tenant's initial catalog is still read at the discard timestamp

#### Scenario: A collected version stays collected after a restart
- **WHEN** a version below the discard timestamp is collected, its deletion is swept and the store is closed and reopened
- **THEN** a read above the deletion's timestamp finds no version of the key

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
