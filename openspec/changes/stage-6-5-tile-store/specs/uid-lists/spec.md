## REMOVED Requirements

### Requirement: Packed lists decode, seek and round-trip
**Reason**: Packed ID lists are gone with the posting lists: a kind 1 adjacency block keeps its neighbours' type
prefixes, sequences and buckets in FOR or DELTA sections, and rows of slots likewise (`STORAGE.md` sections 4.1 and 7.3).
**Migration**: `tile-format`, held to `specification/conformance/storage/`.

### Requirement: Sorted-set operations over UID lists
**Reason**: No stored list remains for these operations to read; compute merges what Scan and Frontier answer
(`PROTOCOL.md` sections 13.13 and 13.21).
**Migration**: `ultragraphdb_core_algo` stays only if compute keeps its sorted-set operations (task 11.2); otherwise
`quad-reads` covers what compute does with rows.

### Requirement: Packed set operations equal the plain ones
**Reason**: Packed lists are removed, so no packed operation remains to compare.
**Migration**: None; see "Sorted-set operations over UID lists".

### Requirement: Decoders seek without panicking or hanging
**Reason**: The packed-list decoder is removed. Every tile decoder refuses malformed bytes with a refusal of
`STORAGE.md` section 19.4 and never panics.
**Migration**: `tile-format`, its decoders' refusals and their fuzz targets.

### Requirement: ID lists pack into the repository's own format
**Reason**: `ultragraphdb_core_codec` is removed; neighbours' IDs are packed per section of an adjacency block
(`STORAGE.md` section 7.3), whose bytes the conformance vectors fix.
**Migration**: `tile-format`.
