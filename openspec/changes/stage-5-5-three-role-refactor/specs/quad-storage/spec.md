## MODIFIED Requirements

### Requirement: Named graphs
Each key SHALL carry its graph as its system ID, 16 big-endian bytes, the default graph taking an encoding of its own
that no system ID takes, and a statement written to one graph SHALL be invisible to reads of every other graph.

#### Scenario: Graphs do not mix
- **WHEN** `<a> <p> <b>` is written to graph `<g1>` and read from graph `<g2>`
- **THEN** the read returns nothing

### Requirement: Every key belongs to one region
Each key SHALL map to exactly one region, a key range: for forward, reverse, value-index, count-index, vector-index and
statistics keys, the keyspace, namespace, graph and predicate; for subject- and object-predicate keys and statement
keys, the keyspace, namespace and graph; for mark keys, referrer keys and dictionary keys (term-dictionary, "The
dictionary is a keyspace without versions"), the keyspace and namespace; for session-table keys (transactions,
"Exactly-once commits through sessions"), the keyspace. A region SHALL be computable from the key alone.

#### Scenario: Two subjects of one predicate share a region
- **WHEN** the forward keys of `knows` for `a` and for `c` in the default graph are mapped to regions
- **THEN** both map to the same region, and the forward key of `name` for `a` maps to another

#### Scenario: Referrer keys share their namespace's region
- **WHEN** the referrer keys of two entities of namespace 7 are mapped to regions
- **THEN** both map to the referrers region of namespace 7, and a referrer key of namespace 8 maps to another

### Requirement: A fresh term's lists are not read
A write that creates a term SHALL read none of the lists keyed by that term's new system ID, since no list can hold it.

#### Scenario: Linking a new subject reads none of its lists
- **WHEN** a write creates `<new>` and writes `<new> <knows> <b>`
- **THEN** the write reads no forward, subject-predicate or statement key of `<new>`

### Requirement: The quad keyspaces
A statement SHALL be stored in these keyspaces of the partition owning its subject, each key starting with a keyspace
byte and the namespace as a u64 big-endian:
- **forward**, keyed by graph, predicate and subject, holding the subject's objects;
- **reverse**, keyed by graph, predicate and object, holding the partition's own subjects, for every object kind,
  literals included, vectors apart;
- **subject predicates**, keyed by graph and subject, holding the predicates the subject has;
- **object predicates**, keyed by graph and object, holding the predicates that point at the object from the
  partition's own subjects, vectors apart;
- **value index**, keyed by graph, predicate, tokenizer and token, holding the partition's own subjects, for the
  tokenizers the catalog declares;
- **count index**, keyed by graph, predicate, direction and count, holding the subjects, for the predicates the catalog
  declares an exact count index on;
- **vector index**, keyed by graph, predicate and node, holding the HNSW lists of a predicate the catalog declares a
  vector index on;
- **statements**, keyed by graph and statement ID, holding the statement and its properties;
- **statistics**, keyed by graph and predicate.

A partition referencing an entity another partition owns SHALL hold, in the **marks** keyspace keyed by the namespace,
the entity's ID and the referring bucket group as 3 big-endian bytes of its 18 bits, the registered mark of that group,
its value the generation its registration's reply named as a u64 big-endian (term-dictionary, "A reference to another
partition's entity registers at its home"). An entity's home SHALL also hold, in the **referrers** keyspace keyed by the
entity's ID, its referrer set, its value the members in group order, each the group's 3 big-endian bytes, the epoch it
was added at as a u64 big-endian and its generation as a u64 big-endian; a referrer set holds at most the 2^10 groups of
a run and never splits (partitions, "An entity's referrer set names where its inbound edges may be"). Graphs,
predicates, subjects and IRI or blank-node objects SHALL appear in keys as their system IDs, 16 big-endian bytes each,
so that a predicate's forward keys sort by subject; a literal object SHALL appear as its encoding.

#### Scenario: A registration mark is kept per object and group
- **WHEN** partition 1's subjects of bucket groups 7 and 9 each reference `<b>`, homed in partition 2, in graphs `<g1>`
  and `<g2>`
- **THEN** partition 1's marks keyspace holds two keys for `<b>`, of groups 7 and 9, each with its generation, and its
  object-predicates keyspace holds no mark

#### Scenario: A triple maintains four lists
- **WHEN** `<a> <knows> <b>` is written to the default graph
- **THEN** the forward list of `knows` for `a` holds `b`, the reverse list of `knows` for `b` holds `a`, the subject
  predicates of `a` hold `knows`, and the object predicates of `b` hold `knows`, all in the partition owning `a`

#### Scenario: A bound literal object is a list read
- **WHEN** `<a> <name> "Ann"` and `<c> <name> "Ann"` are written, `a` and `c` in one partition
- **THEN** that partition's reverse list of `name` for `"Ann"` holds `a` and `c`

#### Scenario: A vector has no reverse entry
- **WHEN** `<a> <embedding> "[0.5, -1]"^^ugdb:float32vector` is written
- **THEN** the forward list of `embedding` for `a` holds the vector, and no reverse or object-predicate key names it

#### Scenario: Forward keys of a predicate sort by subject
- **WHEN** forward keys of one predicate are built for subjects of one type and one bucket with sequences 1, 2, 256
  and 2^40
- **THEN** their byte order equals the subjects' IDs' numeric order

### Requirement: Statement IDs
A statement ID SHALL be a system ID of the `Statement` type, its bucket the bucket of the statement's subject and its
sequence drawn from a block the subject's partition holds, allocated only for a statement that gains a property or
repeats a subject, predicate, object and graph already held. Its forward posting SHALL carry the statement ID, and its
statement record SHALL hold the statement and its properties. Its reifier SHALL be named by a skolem IRI the dictionary
holds (term-dictionary, "IRIs and blank nodes get system IDs"). Removing the statement SHALL remove its record.

#### Scenario: A property allocates a statement ID
- **WHEN** `<a> <knows> <b>` is held without a statement ID and gains the property `<since> 2020`
- **THEN** the statement has a statement ID of type prefix 2 and the bucket of `<a>`, its forward posting carries it, and
  its record holds `<since> 2020`

#### Scenario: Removing a statement removes its record
- **WHEN** a statement with a statement ID is deleted
- **THEN** no statement record of that ID remains
