## REMOVED Requirements

### Requirement: HNSW vectors live in posting lists
**Reason**: Posting lists and the vector-index keyspace are removed with the KV store, and `ultragraphdb_store_hnsw`
with them. The vector index is retired until stage 8, which rebuilds it on adjacency blocks.
**Migration**: A vector stays a forward value of its predicate (`STORAGE.md` section 7.1, kind 2), read back whole; a
read binding a vector object reads the predicate's forward families (`quad-reads`). A vector index declaration is
refused with code 7 (Refused) in this stage (`PROTOCOL.md` section 9.4).

### Requirement: The dead list and the entry point stay true to the data
**Reason**: Retired with the HNSW index until stage 8.
**Migration**: Stage 8 specifies the index on adjacency blocks.

### Requirement: Search returns the nearest neighbours
**Reason**: Retired with the HNSW index until stage 8.
**Migration**: Stage 8 specifies the index on adjacency blocks; no search is served in this stage.

### Requirement: Index parameters
**Reason**: Retired with the HNSW index until stage 8; a vector index declaration is refused in this stage.
**Migration**: `SCHEMA.md` section 6.10's `@index(ann, …)` is stage 8's.

### Requirement: Vectors are indexed in commit order, outside the transaction
**Reason**: Retired with the HNSW index and its indexer until stage 8.
**Migration**: Stage 8 specifies the index on adjacency blocks.

### Requirement: Level draws are a function of the node
**Reason**: Retired with the HNSW index until stage 8.
**Migration**: Stage 8 specifies the index on adjacency blocks.
