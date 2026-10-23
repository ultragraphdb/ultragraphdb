## ADDED Requirements

### Requirement: HNSW vectors live in posting lists
A `float32vector` predicate with an hnsw index SHALL store each vector as a value posting; the entry point as the
8 big-endian bytes of its uid, in the value at uid 1 of `<attr>__vector_entry`; each node's neighbours as one value at
the node's uid under `<attr>__vector_`, holding every level's list as one matrix encoded little-endian; and the deleted
nodes as a JSON array of uids, in the value at uid 1 of `<attr>__vector_dead`, as Dgraph does. Dgraph copies the matrix
in native byte order, which is little-endian on amd64 and arm64.

#### Scenario: The first insert creates the entry point
- **WHEN** the first vector is inserted into an empty index
- **THEN** the entry point names its uid and its neighbour lists exist, empty, at every level

#### Scenario: Deleting a vector marks it dead
- **WHEN** a vector is deleted
- **THEN** its uid is added to the dead list, and later inserts leave it out of the neighbour lists they write

### Requirement: Search returns the nearest neighbours
Searching for the `k` nearest neighbours SHALL use the predicate's metric (euclidean, cosine or dot product), SHALL
honour an `ef` override and a distance threshold, and SHALL return the exact top `k` on sets small enough to fit in
one search.

#### Scenario: Small sets return the exact top k for each metric
- **WHEN** 50 vectors are indexed under each metric and a search asks for the 5 nearest to a query vector
- **THEN** the result equals the 5 nearest by brute force under that metric

#### Scenario: A distance threshold filters results
- **WHEN** a search carries a distance threshold
- **THEN** no returned vector lies farther than the threshold

#### Scenario: An empty index returns nothing
- **WHEN** an index holding no vectors is searched
- **THEN** the index reports an empty-index result, which the caller treats as no neighbours, and the search returns
  an empty result without error

### Requirement: Index parameters
The index SHALL default to 3 levels, an `efConstruction` of 150 and an `efSearch` of 90, and an `exponent` `e`
SHALL set each of `maxLevels`, `efConstruction` and `efSearch` not given explicitly to `e`, `50e` and `30e`.

#### Scenario: The exponent sets the parameters
- **WHEN** an hnsw index is declared with `exponent: "4"`
- **THEN** it uses 4 levels, an `efConstruction` of 200 and an `efSearch` of 120
