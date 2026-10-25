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

### Requirement: The dead list and the entry point stay true to the data
Index maintenance SHALL mark a vector's uid dead whether the vector is deleted by its value or by a star delete,
SHALL leave an overwritten vector's uid live, and SHALL remove a uid from the dead list when its vector is set again. When the entry point has lost its vector, an insert SHALL make the first
node holding a vector the entry point with its neighbour lists kept, and SHALL insert the new vector. Dgraph marks
only a star delete dead, marks an overwritten uid dead for good, never removes a uid from the dead list, and on a
dead entry point wipes the new entry's lists
and drops the insert.

#### Scenario: A delete by value marks the vector dead
- **WHEN** a vector of `emb: float32vector @index(hnsw) .` is deleted by its value
- **THEN** its uid is added to the dead list

#### Scenario: Overwriting a vector does not leave its uid dead
- **WHEN** a vector of `emb: float32vector @index(hnsw) .` is set over the vector it holds
- **THEN** its uid is not in the dead list, and the index still finds it

#### Scenario: A vector deleted and set again is live
- **WHEN** a vector of `emb: float32vector @index(hnsw) .` is deleted and then set again
- **THEN** its uid is not in the dead list, and later inserts keep it in their neighbour lists

#### Scenario: A dead entry point hands over without wiping a node
- **WHEN** the entry point's vector is deleted and another vector is inserted
- **THEN** the first node holding a vector becomes the entry point with its neighbour lists kept, and the new vector
  is inserted

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
SHALL set each of `maxLevels`, `efConstruction` and `efSearch` not given explicitly to `e`, `50e` and `30e`. A schema
change carrying any of the three below 1, directly or through `exponent`, SHALL be refused, and the schema SHALL not
hold the predicate; Dgraph accepts it and panics on search.

#### Scenario: The exponent sets the parameters
- **WHEN** an hnsw index is declared with `exponent: "4"`
- **THEN** it uses 4 levels, an `efConstruction` of 200 and an `efSearch` of 120

#### Scenario: An HNSW option below 1 is refused at the schema change
- **WHEN** `emb: float32vector @index(hnsw(maxLevels:"0")) .` is applied
- **THEN** the change fails with `IndexFactory: hnsw issues this error: 'the vector index option maxLevels must be at
  least 1, got 0'`, and the schema does not hold `emb`
