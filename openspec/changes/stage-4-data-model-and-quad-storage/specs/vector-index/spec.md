## MODIFIED Requirements

### Requirement: HNSW vectors live in posting lists
A predicate the catalog declares with a vector index SHALL store each vector as a literal in its forward list, and its
HNSW lists in the vector-index keyspace under its graph and predicate: the entry point as the 8 big-endian bytes of its
node's UID; each node's neighbours as one value under the node's UID, holding every level's list as one matrix encoded
little-endian; and the deleted nodes as a JSON array of UIDs. Dgraph copies the matrix in native byte order, which is
little-endian on amd64 and arm64.

#### Scenario: The first insert creates the entry point
- **WHEN** the first vector is inserted into an empty index
- **THEN** the entry point names its node's UID and its neighbour lists exist, empty, at every level

#### Scenario: Deleting a vector marks it dead
- **WHEN** a vector is deleted
- **THEN** its node's UID is added to the dead list, and later inserts leave it out of the neighbour lists they write

#### Scenario: Vector lists stay within their graph and predicate
- **WHEN** two predicates in two graphs each hold vectors
- **THEN** each predicate's HNSW lists are under its own graph and predicate, and a search of one finds none of the
  other's nodes
