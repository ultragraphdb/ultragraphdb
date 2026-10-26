## ADDED Requirements

### Requirement: A keyspace without versions
The store SHALL provide a keyspace whose keys carry no version: a point read returns the key's one value, a lookup of an
absent key is answered by fjall's bloom filters, and a write replaces the value. Writes to it SHALL be entries of the
same atomic write batch as versioned writes, so a batch is visible whole or not at all across both, and drops of
everything SHALL clear it. Reads of it SHALL not be bounded by the discard timestamp.

#### Scenario: A versionless write is atomic with versioned writes
- **WHEN** a batch writes a versioned key and a key without a version, and the process crashes during the batch
- **THEN** after reopening, either both are found or neither is

#### Scenario: An absent key is a point read
- **WHEN** a key that was never written is read from the keyspace without versions
- **THEN** the read returns nothing, through one point lookup

#### Scenario: Dropping everything clears it
- **WHEN** keys without versions are written and everything is dropped
- **THEN** none of them is found

## MODIFIED Requirements

### Requirement: Garbage collection below the discard timestamp
Given a discard timestamp `D` and a tombstone horizon `H` at or below it, compaction SHALL drop every version of a key
older than its newest discard-flagged or deletion version at or below `D`, SHALL never drop a deletion, and SHALL keep
every version above `D`. A sweep at one snapshot SHALL remove a deletion at `T` at or below `H` when no version of its
key remains below `T`, and no write SHALL land below a deletion of the same key. `H` SHALL equal `D` unless the store's
owner sets it lower. The compaction filter SHALL keep any item it cannot decode, and until the store has loaded `D` the
filter SHALL drop nothing. `D` SHALL never exceed the oldest open reader's timestamp. A version a prefix drop writes
(the namespace's initial catalog) SHALL carry the fenced flag and SHALL never be collected. A version the
filter collects SHALL stay collected across a restart.

#### Scenario: The initial catalog a drop writes survives garbage collection
- **WHEN** everything in a namespace is dropped, its initial catalog is written at timestamp 1, the discard timestamp is
  raised above the drop and the store compacts
- **THEN** the namespace's initial catalog is still read at the discard timestamp

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
