## ADDED Requirements

### Requirement: Quads are kept in tiles' families
A statement SHALL be kept in the partition owning its subject, in the families of the generic layout (`STORAGE.md`
section 13.4), each family the tiles sharing a family key of the tenant, the graph, the predicate, the kind, the
direction, the storage class, the tokenizer and the bucket group (section 13.1), with the delta above them (section 14):
- the **forward** family of kind 1 for objects with system IDs, IRIs and blank nodes, or of kind 2 for every other
  object, a row per subject keyed by its slot pair, the object its neighbour;
- the **reverse** family of kind 3, a row per object keyed by its term encoding, an IRI's and a blank node's tag `0x01`
  or `0x02` followed by its system ID, the partition's own subjects its neighbours, for every object but a vector,
  which no reverse key names (`PROTOCOL.md` section 11.12);
- an **index** family of kind 4 per tokenizer the catalog declares for the predicate, a row per token (section 5.3),
  the subjects holding it its neighbours, a `count` index among them; a range token's sortable type picked by the
  literal's datatype, and a count token holding its direction and the count, as section 5.3 states (tokenizers,
  "Sortable tokens preserve order"; T48).

Every entry SHALL carry its statement's slot pair or `ABSENT`, and apply SHALL derive the reverse and index entries
from the round's records, never logging them (`STORAGE.md` section 15.2; `PROTOCOL.md` section 12.2). What a partition
records beside its quads SHALL be records of their kinds (`STORAGE.md` section 9.3): a statement's record (kind 18), a
reference's registered mark (kind 6), keyed by the entity's system ID and the referring bucket group as 3 bytes and
holding the generation its registration's reply named, and at the entity's home its referrer set (kind 7), additive,
its fold the union by group keeping each group's greatest generation and earliest epoch (term-dictionary, "A reference
to another partition's entity registers at its home"). No subject-predicates or object-predicates list SHALL be kept: a
read of every predicate of a subject or an object names every predicate of its kind (`STORAGE.md` section 18.5).

#### Scenario: A triple keeps a forward and a reverse entry
- **WHEN** `<a> <knows> <b>` is written to the default graph
- **THEN** the partition owning `a` holds an entry in the forward family of kind 1 of `knows`, row `a`'s slot pair,
  neighbour `b`'s system ID, and one in the reverse family of kind 3, row `b`'s term encoding, neighbour `a`'s slot
  pair, and no other family of `knows` changes

#### Scenario: A bound literal object is a row of the reverse family
- **WHEN** `<a> <name> "Ann"` and `<c> <name> "Ann"` are written, `a` and `c` in one partition
- **THEN** that partition's reverse family of `name` holds the row of `"Ann"`'s term encoding, its neighbours `a`'s and
  `c`'s slot pairs

#### Scenario: A vector has no reverse entry
- **WHEN** `<a> <embedding> "[0.5, -1]"^^ugdb:float32vector` is written
- **THEN** the forward family of kind 2 of `embedding` holds the vector in `a`'s row, and no reverse family holds a row
  keyed by it

#### Scenario: A registration mark is kept per object and group
- **WHEN** partition 1's subjects of bucket groups 7 and 9 each reference `<b>`, homed in partition 2, in graphs `<g1>`
  and `<g2>`
- **THEN** partition 1 holds two mark records for `<b>`, of groups 7 and 9, each with its generation

#### Scenario: Two tenants sharing a run keep their tiles apart
- **WHEN** tenants 7 and 8 share a bucket run, and each writes `<a> <p> <b>`, whose subject takes one bucket in both
- **THEN** each statement's entries lie in families whose family keys hold its own tenant, a read of tenant 7 returns
  none of tenant 8's, and the partition owning the bucket holds both

## MODIFIED Requirements

### Requirement: Named graphs
Each family SHALL carry its graph in its family key as the graph's system ID, the default graph as 16 zero bytes, which
no system ID is (`STORAGE.md` sections 13.1 and 20), and a statement written to one graph SHALL be invisible to reads
of every other graph.

#### Scenario: Graphs do not mix
- **WHEN** `<a> <p> <b>` is written to graph `<g1>` and read from graph `<g2>`
- **THEN** the read returns nothing

### Requirement: Statement IDs
A statement ID SHALL be a system ID of the `Statement` type, its bucket the bucket of the statement's subject and its
sequence drawn from a block the subject's partition holds, allocated only for a statement that gains a property or
repeats a subject, predicate, object and graph already held. Its forward and its reverse entries SHALL both carry the
statement ID's slot pair, apply deriving both from the round's records on apply and on replay, so a reverse read names
each parallel edge's reifier from the row it reads. A parallel edge SHALL give the statement already held an ID of its
own in the same write. Its statement record (`STORAGE.md` section 9.3, kind 18), keyed by the statement's system ID
alone, SHALL hold the quad and its properties. Its reifier SHALL be named by a skolem IRI the dictionary holds
(term-dictionary, "IRIs and blank nodes get system IDs"). Removing the statement SHALL remove its record. The partition
SHALL refuse, at the write, a statement that would hold an object under more statement IDs than
`statements_per_object_max`, 16,000, with code 6 (TooLarge), `detail` the bound, ending the transaction, so one object
record of an Entries row fits a frame at the floor of `max_frame` in a forward and a reverse row alike.

#### Scenario: A property allocates a statement ID
- **WHEN** `<a> <knows> <b>` is held without a statement ID and gains the property `<since> 2020`
- **THEN** the statement has a statement ID of type prefix 2 and the bucket of `<a>`, its forward and reverse entries
  carry it, and its record holds `<since> 2020`

#### Scenario: Removing a statement removes its record
- **WHEN** a statement with a statement ID is deleted
- **THEN** no statement record of that ID is visible at the delete's version or after it

#### Scenario: A parallel edge gives the held statement an ID of its own
- **WHEN** `<a> <knows> <b>` is held without a statement ID, and a write inserts a parallel edge of it
- **THEN** the object is held under two statement IDs, one for each statement, in ascending order, in `a`'s forward row
  of `knows` and `b`'s reverse row of `knows` alike, and neither row holds `<b>` without an ID

#### Scenario: Replay derives both entries
- **WHEN** a round holding a statement's first property and a parallel edge is applied, the replica's grid and delta
  are discarded, and the log is replayed into an empty replica
- **THEN** the forward and reverse entries hold the same statement IDs they held after the first apply

#### Scenario: An object past its statement bound is refused
- **WHEN** `<a> <knows> <b>` is held under `statements_per_object_max` statement IDs, and a transaction inserts one more
  parallel edge of it
- **THEN** the Write is refused with code 6, `detail` the bound, on the transaction's stream, the transaction ends, and
  the object's entries are unchanged

### Requirement: Statistics are kept by deltas
Each round SHALL write one statistics record (`STORAGE.md` section 9.3, kind 9) per graph and predicate it changed, at
its last commit version, holding the round's change in section 9.4's format: a statement count, a count per object kind
and per literal datatype, and the registers estimating distinct subjects and objects. Execution SHALL compute the
change, the round SHALL carry it in its derivations section, and apply SHALL write it as carried, reading no prior
value (sections 15.1 and 15.4); a predicate's statistics at a version SHALL be the merge of its records visible there,
and compaction SHALL fold them. The statement count and the counts per kind and datatype SHALL be exact; the distinct
estimates MAY be approximate.

#### Scenario: Inserts raise the statement count
- **WHEN** 100 distinct statements of `knows` are written to the default graph
- **THEN** the statistics of `knows` report 100 statements

#### Scenario: A statement written twice is counted once
- **WHEN** `<a> <knows> <b>` is written in one commit and again in a later one
- **THEN** the statistics of `knows` report 1 statement

#### Scenario: Datatypes seen are counted
- **WHEN** `<a> <age> 30` and `<b> <age> "unknown"` are written
- **THEN** the statistics of `age` report one `xsd:integer` and one `xsd:string` literal

#### Scenario: Apply writes the round's change as carried
- **WHEN** a round inserting 3 statements of `knows` and deleting 1 is applied, and the statistics of `knows` were 10
  before it
- **THEN** apply writes one statistics record of count 2 at the round's last commit version, and a read there reports
  12

### Requirement: A fresh term's lists are not read
A write that creates a term SHALL read none of the rows or records keyed by that term's new system ID, since no entry
can hold it.

#### Scenario: Linking a new subject reads none of its rows
- **WHEN** a write creates `<new>` and writes `<new> <knows> <b>`
- **THEN** the write reads no forward row, statement record or other record keyed by `<new>`'s ID

## REMOVED Requirements

### Requirement: The quad keyspaces
**Reason**: The KV keyspaces are removed with the KV store. Quads are kept in the generic layout's families of tiles,
and the subject-predicates and object-predicates lists go: a request naming every predicate of a kind reads a subject's
or an object's rows of all predicates (`STORAGE.md` sections 13.4 and 18.5).
**Migration**: "Quads are kept in tiles' families"; marks and referrer sets are records of kinds 6 and 7, statements'
records kind 18 and statistics kind 9 (`STORAGE.md` section 9.3).

### Requirement: Every key belongs to one region
**Reason**: Regions are KV key ranges; a tile belongs to exactly one family, and the family key (`STORAGE.md` section
13.1) is computable from the tile key alone, its tenant, graph, predicate, kind, class, tokenizer and bucket group
included.
**Migration**: `tile-format` (tile keys) and `tile-engine` (families of the generic layout).

### Requirement: Lists split as in stage 2
**Reason**: Lists and their split keys are removed; compaction cuts a family's tiles at `tile_entries_max` entries or
`tile_bytes_max` bytes, a row spanning blocks (`STORAGE.md` sections 7.5 and 16.3).
**Migration**: `tile-engine`, compaction.
