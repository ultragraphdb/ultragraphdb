# quad-storage Specification

## Purpose
Quads on posting lists: the quad keyspaces and their key order, the forward, reverse and predicate lists, literal
objects, statement records and statistics kept by deltas.
## Requirements
### Requirement: The quad keyspaces
A statement SHALL be stored in these keyspaces, each key starting with a keyspace byte and the namespace as a u64
big-endian:
- **forward**, keyed by graph, predicate and subject, holding the subject's objects;
- **reverse**, keyed by graph, predicate and object, holding the subjects, for every object kind, literals included,
  vectors apart;
- **subject predicates**, keyed by graph and subject, holding the predicates the subject has;
- **object predicates**, keyed by graph and object, holding the predicates that point at the object, vectors apart;
- **value index**, keyed by graph, predicate, tokenizer and token, holding the subjects, for the tokenizers the catalog
  declares;
- **count index**, keyed by graph, predicate, direction and count, holding the subjects, for the predicates the catalog
  declares an exact count index on;
- **vector index**, keyed by graph, predicate and node, holding the HNSW lists of a predicate the catalog declares a
  vector index on;
- **statements**, keyed by graph and statement ID, holding the statement and its properties;
- **statistics**, keyed by graph and predicate.

Graphs, predicates, subjects and IRI or blank-node objects SHALL appear in keys as their UIDs, big-endian, so that a
predicate's forward keys sort by subject; a literal object SHALL appear as its encoding.

#### Scenario: A triple maintains four lists
- **WHEN** `<a> <knows> <b>` is written to the default graph
- **THEN** the forward list of `knows` for `a` holds `b`, the reverse list of `knows` for `b` holds `a`, the subject
  predicates of `a` hold `knows`, and the object predicates of `b` hold `knows`

#### Scenario: A bound literal object is a list read
- **WHEN** `<a> <name> "Ann"` and `<c> <name> "Ann"` are written
- **THEN** the reverse list of `name` for `"Ann"` holds `a` and `c`

#### Scenario: A vector has no reverse entry
- **WHEN** `<a> <embedding> "[0.5, -1]"^^ugdb:float32vector` is written
- **THEN** the forward list of `embedding` for `a` holds the vector, and no reverse or object-predicate key names it

#### Scenario: Forward keys of a predicate sort by subject
- **WHEN** forward keys of one predicate are built for subjects with UIDs 1, 2, 256 and 2^40
- **THEN** their byte order equals the UIDs' numeric order

### Requirement: Named graphs
Each key SHALL carry its graph as a UID, the default graph being 0, and a statement written to one graph SHALL be
invisible to reads of every other graph.

#### Scenario: Graphs do not mix
- **WHEN** `<a> <p> <b>` is written to graph `<g1>` and read from graph `<g2>`
- **THEN** the read returns nothing

### Requirement: Statement IDs
A statement ID SHALL be allocated per namespace from a sequence distinct from UIDs, only for a statement that gains a
property or repeats a subject, predicate, object and graph already held. Its forward posting SHALL carry the statement
ID, and its statement record SHALL hold the statement and its properties. Removing the statement SHALL remove its record.

#### Scenario: A property allocates a statement ID
- **WHEN** `<a> <knows> <b>` is held without a statement ID and gains the property `<since> 2020`
- **THEN** the statement has a statement ID, its forward posting carries it, and its record holds `<since> 2020`

#### Scenario: Removing a statement removes its record
- **WHEN** a statement with a statement ID is deleted
- **THEN** no statement record of that ID remains

### Requirement: Every key belongs to one region
Each key SHALL map to exactly one region, a key range: for forward, reverse, value-index, count-index, vector-index and
statistics keys, the keyspace, namespace, graph and predicate; for subject- and object-predicate keys and statement
keys, the keyspace, namespace and graph. A region SHALL be computable from the key alone.

#### Scenario: Two subjects of one predicate share a region
- **WHEN** the forward keys of `knows` for `a` and for `c` in the default graph are mapped to regions
- **THEN** both map to the same region, and the forward key of `name` for `a` maps to another

### Requirement: Statistics are kept by deltas
Each write SHALL add deltas to the statistics of its graph and predicate: a statement count, estimates of distinct
subjects and objects, and a count per object kind and per literal datatype seen. No write SHALL read statistics. The
statement count and the counts per kind and datatype SHALL be exact; the distinct estimates MAY be approximate.

#### Scenario: Inserts raise the statement count
- **WHEN** 100 distinct statements of `knows` are written to the default graph
- **THEN** the statistics of `knows` report 100 statements

#### Scenario: A statement written twice is counted once
- **WHEN** `<a> <knows> <b>` is written in one commit and again in a later one
- **THEN** the statistics of `knows` report 1 statement

#### Scenario: Datatypes seen are counted
- **WHEN** `<a> <age> 30` and `<b> <age> "unknown"` are written
- **THEN** the statistics of `age` report one `xsd:integer` and one `xsd:string` literal

### Requirement: Lists split as in stage 2
Every list keyspace SHALL split a list that grows past the split threshold into parts, as stage 2's posting lists do,
a part's key being the list's key with the split flag set in its keyspace byte and the part's start appended.

#### Scenario: A large reverse list splits
- **WHEN** a reverse list grows past the split threshold and is rolled up
- **THEN** it is stored as parts, and reading it returns every subject in order

### Requirement: A fresh term's lists are not read
A write that creates a term SHALL read none of the lists keyed by that term's new UID, since no list can hold it.

#### Scenario: Linking a new subject reads none of its lists
- **WHEN** a write creates `<new>` and writes `<new> <knows> <b>`
- **THEN** the write reads no forward, subject-predicate or statement key of `<new>`

