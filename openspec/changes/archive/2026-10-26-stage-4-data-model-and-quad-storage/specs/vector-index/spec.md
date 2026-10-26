## MODIFIED Requirements

### Requirement: HNSW vectors live in posting lists
A predicate the catalog declares with a vector index SHALL store each vector as a `ugdb:float32vector` literal, encoded
by value, in its forward list, and the HNSW search SHALL read the elements from that encoding. It SHALL store its
HNSW lists in the vector-index keyspace under its graph and predicate: the entry point as the 8 big-endian bytes of its
node's UID; each node's neighbours as one value under the node's UID, holding every level's list as one matrix encoded
little-endian; and the deleted nodes as a JSON array of UIDs. Dgraph copies the matrix in native byte order, which is
little-endian on amd64 and arm64. A vector whose dimension differs from that of the vectors the index holds SHALL be
refused, writing nothing of its changeset.

#### Scenario: A vector of another dimension is refused
- **WHEN** a vector index on `embedding` holds 768-element vectors and a 512-element vector is written to `embedding`
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

### Requirement: The dead list and the entry point stay true to the data
Index maintenance SHALL mark a vector's node dead when its vector is deleted, by its value or by a delete of the
pattern, SHALL leave an overwritten vector's node live, and SHALL remove a node from the dead list when its vector is
set again. When the entry point has lost its vector, an insert SHALL make the first node holding a vector the entry
point with its neighbour lists kept, and SHALL insert the new vector. Dgraph marks only a star delete dead, marks an
overwritten uid dead for good, never removes a uid from the dead list, and on a dead entry point wipes the new entry's
lists and drops the insert.

#### Scenario: A delete by value marks the vector dead
- **WHEN** a vector of `emb`, declared single-valued with a vector index, is deleted by its value
- **THEN** its node's UID is added to the dead list

#### Scenario: Overwriting a vector does not leave its uid dead
- **WHEN** a vector of `emb`, declared single-valued with a vector index, is written over the vector it holds
- **THEN** its node's UID is not in the dead list, and a search for the new vector finds it

#### Scenario: A vector deleted and set again is live
- **WHEN** a vector of `emb`, declared single-valued with a vector index, is deleted and then written again
- **THEN** its node's UID is not in the dead list, and later inserts keep it in their neighbour lists

#### Scenario: A dead entry point hands over without wiping a node
- **WHEN** the entry point's vector is deleted and another vector is inserted
- **THEN** the first node holding a vector becomes the entry point with its neighbour lists kept, and the new vector
  is inserted

### Requirement: Index parameters
The index SHALL default to 3 levels, an `efConstruction` of 150 and an `efSearch` of 90, and an `exponent` `e`
SHALL set each of `maxLevels`, `efConstruction` and `efSearch` not given explicitly to `e`, `50e` and `30e`. A
declaration carrying any of the three below 1, directly or through `exponent`, SHALL be refused, and the catalog SHALL
not hold the predicate; Dgraph accepts it and panics on search.

#### Scenario: The exponent sets the parameters
- **WHEN** an hnsw index is declared with `exponent: "4"`
- **THEN** it uses 4 levels, an `efConstruction` of 200 and an `efSearch` of 120

#### Scenario: An HNSW option below 1 is refused at the declaration
- **WHEN** `emb` is declared with a vector index whose `maxLevels` is `0`
- **THEN** the declaration fails with `IndexFactory: hnsw issues this error: 'the vector index option maxLevels must
  be at least 1, got 0'`, and the catalog does not hold `emb`
