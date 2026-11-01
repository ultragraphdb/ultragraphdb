## MODIFIED Requirements

### Requirement: HNSW vectors live in posting lists
A predicate the catalog declares with a vector index SHALL store each vector as a `ugdb:float32vector` literal, encoded
by value, in its forward list, and the HNSW search SHALL read the elements from that encoding. It SHALL store its
HNSW lists in the vector-index keyspace under its graph and predicate: the entry point as the 8 big-endian bytes of its
node's UID; each node's neighbours as one value under the node's UID, holding every level's list as one matrix encoded
little-endian; and the deleted nodes as a JSON array of UIDs. Dgraph copies the matrix in native byte order, which is
little-endian on amd64 and arm64. A vector whose dimension differs from the dimension its index declaration states
SHALL be refused, writing nothing of its changeset.

#### Scenario: A vector of another dimension is refused
- **WHEN** a vector index on `embedding` is declared with dimension 768 and a 512-element vector is written to
  `embedding`
- **THEN** the write fails with an error naming `embedding`, 768 and 512, and nothing of its changeset is held

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

### Requirement: Index parameters
A vector index declaration SHALL state its dimension, at least 1. The index SHALL default to 3 levels, an
`efConstruction` of 150 and an `efSearch` of 90, and an `exponent` `e` SHALL set each of `maxLevels`, `efConstruction`
and `efSearch` not given explicitly to `e`, `50e` and `30e`. A declaration without a dimension, or carrying any of the
three below 1, directly or through `exponent`, SHALL be refused, and the catalog SHALL not hold the predicate; Dgraph
accepts it and panics on search.

#### Scenario: The exponent sets the parameters
- **WHEN** an hnsw index is declared with dimension 4 and `exponent: "4"`
- **THEN** it uses 4 levels, an `efConstruction` of 200 and an `efSearch` of 120

#### Scenario: An HNSW option below 1 is refused at the declaration
- **WHEN** `emb` is declared with a vector index of dimension 4 whose `maxLevels` is `0`
- **THEN** the declaration fails with `IndexFactory: hnsw issues this error: 'the vector index option maxLevels must
  be at least 1, got 0'`, and the catalog does not hold `emb`

#### Scenario: A declaration without a dimension is refused
- **WHEN** `emb` is declared with a vector index and no dimension
- **THEN** the declaration fails with an error naming `emb` and the missing dimension, and the catalog does not hold
  `emb`

## ADDED Requirements

### Requirement: Vectors are indexed in commit order, outside the transaction
A transaction writing a vector SHALL commit it in its forward list without reading or writing the HNSW lists. An indexer
SHALL insert committed vectors into the HNSW lists in commit order and record the version it has indexed. A search SHALL
return only nodes whose vector is visible at its snapshot; a vector committed but not yet indexed SHALL be absent from
searches until the indexer reaches it.

#### Scenario: Concurrent vector writes do not conflict
- **WHEN** eight transactions concurrently write vectors to `emb` for eight different nodes
- **THEN** all eight commit, and once the indexer has passed their versions a search finds each of them

#### Scenario: A deleted vector is not returned
- **WHEN** a vector is deleted and a search runs before the indexer has processed the delete
- **THEN** the search does not return its node

### Requirement: Level draws are a function of the node
A node's HNSW level SHALL be drawn from a hash of the index's seed, the namespace, the predicate and the node's UID, so
inserting the same vectors in any order draws the same levels.

#### Scenario: Insertion order does not change levels
- **WHEN** the same ten vectors are indexed once in one order and once in the reverse order
- **THEN** each node is drawn the same level both times
