## REMOVED Requirements

### Requirement: A read merges deltas over the complete list
**Reason**: Posting lists are gone: a relation's rows are adjacency blocks and the delta above them, and a read at
version `v` merges the tile entries and the delta entries visible at `v` (`STORAGE.md` sections 7, 14.4 and 18.2).
**Migration**: `storage-lanes`, what a read sees; `tile-engine`, visibility.

### Requirement: Posting operations and identities
**Reason**: An entry is a row key, a neighbour and a statement (`STORAGE.md` section 7.2); execution never logs an
insert of an entry already visible nor a delete of one not visible (section 14.4), and a literal neighbour is its term
encoding, so no fingerprint orders or identifies it.
**Migration**: `quad-storage`, "Quads are kept in tiles' families"; `tile-engine`, apply.

### Requirement: Delete-all markers
**Reason**: A delete of a statement without a statement ID deletes every visible entry of its row key and neighbour
(`STORAGE.md` section 14.4); drops and purges are markers of their own (section 15.5). `_STAR_ALL` does not exist.
**Migration**: `tile-engine`, visibility, drops and purges.

### Requirement: Rollups
**Reason**: Compaction rewrites due tiles at round boundaries, folding at the logged fold horizon (`STORAGE.md`
sections 14.5 and 16); no rollup remains.
**Migration**: `tile-engine`, compaction.

### Requirement: Split lists
**Reason**: A row spans adjacency blocks of its family, cut between entries by compaction (`STORAGE.md` sections 7.5
and 16.3); no split key exists.
**Migration**: `tile-format` and `tile-engine`.

### Requirement: Posting lists are stored in the repository's own format
**Reason**: `ultragraphdb_core_codec` and `ultragraphdb_store_posting` are removed; every stored byte is a tile's,
checksummed per tile (`STORAGE.md` sections 6 to 11).
**Migration**: `tile-format`, held to `specification/conformance/storage/`.
