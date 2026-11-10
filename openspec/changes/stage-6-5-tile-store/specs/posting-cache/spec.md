## REMOVED Requirements

### Requirement: The cache never returns a stale list
**Reason**: No posting list is cached. Tiles are immutable and named by their references (`STORAGE.md` section 6.4),
so a cached tile is never stale; what tiles lack is in the delta, which every read merges (section 18.2).
**Migration**: `storage-lanes`; a request whose tiles are not in the cache restarts its attempt once they are read
(T33).

### Requirement: The cache changes with the write it reflects
**Reason**: A commit changes the delta, never a cached tile; compaction writes new tiles out of place (`STORAGE.md`
sections 12.4 and 16).
**Migration**: `tile-engine`, apply and compaction.

### Requirement: A reader fills the cache only with a current list
**Reason**: A tile read by its reference is checked against its checksum and cannot be out of date (`STORAGE.md`
sections 6.4 and 6.5); no writer lock or epoch guards a fill.
**Migration**: `tile-grid`, reading a tile.

### Requirement: Schema changes and drops clear the cache
**Reason**: Drops and builds are logged engine operations that change the directory and the delta (`STORAGE.md`
sections 15.5 and 16.7); a dropped family's tiles leave the directory and no cache epoch exists.
**Migration**: `tile-engine`, drops, purges and builds; `schema-changes`.
