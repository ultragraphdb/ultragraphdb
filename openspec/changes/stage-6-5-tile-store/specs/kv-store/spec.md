## REMOVED Requirements

### Requirement: Versions are part of the key
**Reason**: The tile store (`specification/storage/STORAGE.md`) replaces fjall: a version is an entry's created and
deleted version inside a tile or the delta (sections 7.2, 9.1 and 14), never a suffix of a key, and Badger's and fjall's
key limits give way to `value_bytes_max` (section 3.3).
**Migration**: `tile-format` holds the bytes of entries and their versions, `tile-engine` the delta and visibility;
a value past `value_bytes_max` is refused with code 6 (TooLarge). KV directories written before this change are not
readable.

### Requirement: Reads at a timestamp
**Reason**: Reads take a version in a lane (`STORAGE.md` section 18), and the discard timestamp gives way to the logged
fold horizon (section 14.2).
**Migration**: `storage-lanes` states what a read at a version sees; a read below the latest logged horizon answers
code 3 (TooOld).

### Requirement: Snapshot scans
**Reason**: fjall snapshots are gone; a request reads one version of tiles and the delta (`STORAGE.md` sections 18.1
and 18.2), restarting its attempt on a cache miss (T33).
**Migration**: `storage-lanes` and `quad-reads`, "Storage's read kernel answers what compute scans".

### Requirement: Atomic write batches
**Reason**: The log is the only stream synced per commit; apply turns a committed round into delta changes, and a
checkpoint publishes tiles under a superblock (`STORAGE.md` sections 2, 15 and 17). No write batch and no fjall journal
remain.
**Migration**: `tile-engine` (apply) and `tile-grid` (checkpoints and recovery).

### Requirement: Garbage collection below the discard timestamp
**Reason**: Compaction folds versions at or below the logged fold horizon `H`, deterministically on every replica
(`STORAGE.md` sections 14.2, 14.5 and 16); no discard timestamp, tombstone horizon or compaction filter remains.
**Migration**: `tile-engine`, folding and compaction.

### Requirement: A keyspace without versions
**Reason**: Every record is in a record run of its kind (`STORAGE.md` section 9), versioned by its storage class
(section 14.3); the dictionary's kinds 2 and 3 are class I and kind 1 class M.
**Migration**: `term-dictionary`, "The dictionary is kept in record runs of kinds 1 to 3"; the tile cache replaces
the 64 MiB value cache.

### Requirement: Prefix drops and banned tenants
**Reason**: Drops are logged engine operations: a drop marker (kind 14) under which reads treat the covered entries as
absent and compaction removes them (`STORAGE.md` section 15.5); no lsm-tree removal, drop record or banned set remains.
**Migration**: `schema-changes` and `tile-engine`, drops and purges.
