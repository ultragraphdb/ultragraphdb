## MODIFIED Requirements

### Requirement: HNSW vectors live in posting lists
A predicate the catalog declares with a vector index SHALL store each vector as a `ugdb:float32vector` literal, encoded
by value, in its forward list, and the HNSW search SHALL read the elements from that encoding. It SHALL store its
HNSW lists in the vector-index keyspace under its graph and predicate: the entry point as the 16 big-endian bytes of its
node's system ID; each node's neighbours as one value under the node's ID, holding every level's list as one matrix of
IDs encoded little-endian; and the deleted nodes as a JSON array of IDs, each as 32 lowercase hexadecimal digits. A
vector whose dimension differs from the dimension its index declaration states SHALL be refused, writing nothing of its
changeset.

#### Scenario: A vector of another dimension is refused
- **WHEN** a vector index on `embedding` is declared with dimension 768 and a 512-element vector is written to
  `embedding`
- **THEN** the write fails with an error naming `embedding`, 768 and 512, and nothing of its changeset is held

#### Scenario: The first insert creates the entry point
- **WHEN** the first vector is inserted into an empty index
- **THEN** the entry point names its node's system ID and its neighbour lists exist, empty, at every level

#### Scenario: Deleting a vector marks it dead
- **WHEN** a vector is deleted
- **THEN** its node's ID is added to the dead list as 32 hexadecimal digits, and later inserts leave it out of the
  neighbour lists they write

#### Scenario: Vector lists stay within their graph and predicate
- **WHEN** two predicates in two graphs each hold vectors
- **THEN** each predicate's HNSW lists are under its own graph and predicate, and a search of one finds none of the
  other's nodes
