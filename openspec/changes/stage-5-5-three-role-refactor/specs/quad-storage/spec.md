## MODIFIED Requirements

### Requirement: Named graphs
Each key SHALL carry its graph as its system ID, 16 big-endian bytes, the default graph taking an encoding of its own
that no system ID takes, and a statement written to one graph SHALL be invisible to reads of every other graph.

#### Scenario: Graphs do not mix
- **WHEN** `<a> <p> <b>` is written to graph `<g1>` and read from graph `<g2>`
- **THEN** the read returns nothing

### Requirement: Every key belongs to one region
Each key SHALL map to exactly one region, a key range: for forward, reverse, value-index, count-index, vector-index and
statistics keys, the keyspace, tenant, graph and predicate; for subject- and object-predicate keys and statement
keys, the keyspace, tenant and graph; for mark keys, referrer keys and dictionary keys (term-dictionary, "The
dictionary is a keyspace without versions"), the keyspace and tenant; for session-table keys (transactions,
"Exactly-once commits through sessions"), the keyspace. A region SHALL be computable from the key alone.

#### Scenario: Two subjects of one predicate share a region
- **WHEN** the forward keys of `knows` for `a` and for `c` in the default graph are mapped to regions
- **THEN** both map to the same region, and the forward key of `name` for `a` maps to another

#### Scenario: Referrer keys share their tenant's region
- **WHEN** the referrer keys of two entities of tenant 7 are mapped to regions
- **THEN** both map to the referrers region of tenant 7, and a referrer key of tenant 8 maps to another

#### Scenario: Two tenants sharing a run keep their keys apart
- **WHEN** tenants 7 and 8 share a bucket run, and each writes `<a> <p> <b>`, whose subject takes one bucket in both
- **THEN** each statement's keys hold its own tenant in bytes 1 to 8, a read of tenant 7 returns none of tenant 8's, and
  the partition owning the bucket holds both

### Requirement: A fresh term's lists are not read
A write that creates a term SHALL read none of the lists keyed by that term's new system ID, since no list can hold it.

#### Scenario: Linking a new subject reads none of its lists
- **WHEN** a write creates `<new>` and writes `<new> <knows> <b>`
- **THEN** the write reads no forward, subject-predicate or statement key of `<new>`

### Requirement: The quad keyspaces
A statement SHALL be stored in these keyspaces of the partition owning its subject, each key starting with a keyspace
byte and the tenant as a u64 big-endian:
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

A partition referencing an entity another partition owns SHALL hold, in the **marks** keyspace keyed by the tenant,
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
repeats a subject, predicate, object and graph already held. Its forward and its reverse posting SHALL both carry the
statement ID, `set_statement_ids` writing both and replay deriving both, so a reverse read names each parallel edge's
reifier from the list it scans. A parallel edge SHALL give the statement already held an ID of its own in the same
write. Its statement record SHALL hold the statement and its properties. Its reifier SHALL be named by a skolem IRI the
dictionary holds (term-dictionary, "IRIs and blank nodes get system IDs"). Removing the statement SHALL remove its
record. The partition SHALL refuse, at the write, a statement that would hold an object under more statement IDs than
`statements_per_object_max`, 16,000, with code 6 (TooLarge), `detail` the bound, ending the transaction, so a key's
object record fits a frame at the floor of `max_frame` in a forward and a reverse list alike.

#### Scenario: A property allocates a statement ID
- **WHEN** `<a> <knows> <b>` is held without a statement ID and gains the property `<since> 2020`
- **THEN** the statement has a statement ID of type prefix 2 and the bucket of `<a>`, its forward and reverse postings
  carry it, and its record holds `<since> 2020`

#### Scenario: Removing a statement removes its record
- **WHEN** a statement with a statement ID is deleted
- **THEN** no statement record of that ID remains

#### Scenario: A parallel edge gives the held statement an ID of its own
- **WHEN** `<a> <knows> <b>` is held without a statement ID, and a write inserts a parallel edge of it
- **THEN** the object is held under two statement IDs, one for each statement, in ascending order, in the forward list
  of `knows` for `a` and the reverse list of `knows` for `b` alike, and neither list holds `<b>` without an ID

#### Scenario: Replay derives both postings
- **WHEN** a round holding a statement's first property and a parallel edge is applied, the store is discarded, and the
  round's record is replayed into an empty store
- **THEN** the forward and reverse postings hold the same statement IDs they held after the first apply

#### Scenario: An object past its statement bound is refused
- **WHEN** `<a> <knows> <b>` is held under `statements_per_object_max` statement IDs, and a transaction inserts one more
  parallel edge of it
- **THEN** the Write is refused with code 6, `detail` the bound, on the transaction's stream, the transaction ends, and
  the object's postings are unchanged
