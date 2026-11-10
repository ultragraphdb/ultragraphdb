## MODIFIED Requirements

### Requirement: IRIs and blank nodes get system IDs
Each IRI and each skolemized blank node, other than a reifier's skolem IRI, SHALL map, per tenant, to one system ID, never reused. One naming no instance of a declared type SHALL take the
`Resource` type: an IRI's bucket computed from the tenant and the IRI's canonical encoding, a blank node's the one its
skolem IRI states, and its sequence drawn from a block its home holds, but for a predicate the catalog version in
effect declares, whose ID is the version's ("A governed tenant's declared predicates resolve from the catalog
version"). An IRI naming an instance of a declared type
SHALL map to an ID of that instance's concrete type's prefix, its bucket computed from its key space and key or stated
by its handle (ugs-identity, "Keyed instances are named by an IRI minted from the key", "Keyless instances are named by
a handle"). The same IRI SHALL
always map to the same ID, and its dictionary entries SHALL be held in its home, the partition owning its bucket.

A new blank node's skolem IRI SHALL be the tenant's skolem base, `https://w3id.org/ultragraphdb/ns/tenant/{n}/` with `{n}` the tenant in decimal, followed
by a bucket and 104 random bits, as `https://w3id.org/ultragraphdb/ns/tenant/{n}/.well-known/genid/{bucket}-{random}`, the bucket in 6 and the random bits
in 26 lowercase hexadecimal digits. The
partition minting it SHALL draw both from its injected random source: the bucket from those it owns in the tenant's
run, the random bits uniformly. A skolem IRI that names an IRI the dictionary already holds SHALL be drawn again. The
skolem IRI SHALL hold no part of the system ID beyond its bucket. A transaction whose first write names only new blank
nodes SHALL bind to a partition the frontend chooses in turn.

A statement ID's reifier SHALL be named by a skolem IRI minted as a blank node's: the statement's bucket, the bucket of
its subject, and 104 random bits from the subject's partition's injected random source, drawn again when the IRI is
already held. The dictionary SHALL hold that IRI as the reifier's name, mapping it to the statement ID and back, so a
client addresses the statement's properties, or one of several parallel edges, through it.

#### Scenario: A statement ID's reifier is a skolem IRI
- **WHEN**, in a tenant with no schema, `<a> <knows> <b>` gains the property `<since> 2020` and so receives a statement
  ID
- **THEN** the dictionary holds a skolem IRI `…/tenant/{n}/.well-known/genid/{bucket}-{random}` for it, whose bucket is `<a>`'s,
  mapping to the statement ID and back, and a write naming that IRI as the reifier adds properties to the statement

#### Scenario: An IRI keeps its ID
- **WHEN** `<a>` is written in two separate commits
- **THEN** both name the same system ID

#### Scenario: Transactions naming only new blank nodes bind in turn
- **WHEN** three transactions whose first writes name only new blank nodes arrive over three partitions
- **THEN** the frontend binds them to partitions 1, 2 and 3 in turn, and each blank node's bucket is one its partition
  owns

#### Scenario: An IRI's bucket is its own
- **WHEN** a new IRI receives a system ID in a tenant with no schema
- **THEN** its type prefix is 1 and its bucket equals the bucket computed from its tenant and canonical encoding

#### Scenario: A blank node's skolem IRI names its bucket and nothing else
- **WHEN** partition 1, owning the first 21,846 buckets of tenant 0's run, mints a blank node with its random source
  seeded to a recorded value
- **THEN** the skolem IRI is the tenant's skolem base, ending `/tenant/0/`, `.well-known/genid/`, 6 hexadecimal digits naming one of those
  buckets, `-` and 26 hexadecimal digits; the node's ID carries that bucket; and the same seed mints the same IRI

#### Scenario: An IRI of a declared type's instance maps to that type's prefix
- **WHEN** a `Company` with `id` `ORG:acme` is created under Gen5
- **THEN** the dictionary maps `https://example.org/deep_brain#Node/ORG%3Aacme` to an ID whose type prefix is
  `Company`'s, in the bucket computed from `Node`'s prefix and the canonical key

### Requirement: The dictionary is kept in record runs of kinds 1 to 3
The dictionary SHALL be three kinds of record (`STORAGE.md` section 9.3), read by Scan ranges of space 1 (`PROTOCOL.md`
section 13.13): term to ID (kind 1), its key the XXH3-64 of the term's name bytes as a u64 followed by the name bytes,
a name's bytes being its kind, 1 an IRI, 2 a blank node's skolem IRI, 3 a reifier's name or 4 a handle IRI, and then
its UTF-8 text, its value the system ID, a `Resource` ID for an IRI naming no instance of a declared type, the
instance's ID, of its concrete type's prefix, for one that does, and for a reifier's name a statement ID of the
built-in `Statement` prefix or of its declared relation's; ID to term (kind 2), its key the system ID, its value the ID the term's
re-creation replaced, 16 zero bytes for none, then the name bytes; and hash to text (kind 3), its key the 16-byte hash a
term encoding holds (section 5.1), its value the text. Kinds 2 and 3 SHALL be of storage class I, never versioned
beyond their created version, and kind 1 of class M, so a term to ID record can be deleted at a version (section 14.3;
T48, C16). The dictionary SHALL be naming state only: whether an entity exists, and which types it has, SHALL be read
from committed data, never from a dictionary record, so a record created for a write that then aborted names nothing
that exists. A hash to text record SHALL be written blind: equal texts write equal keys and values. An ID to term
record SHALL never change once written, since IDs are never reused. A term re-created after its delete SHALL delete its
old term to ID record at the re-creation's version, so a read below it still resolves the old ID, and its new ID to
term record SHALL name the ID it replaced (section 15.3). A copy of another partition's term to ID or ID to term record
SHALL be held in the family of bucket group `ABSENT`, the replica's own, which no bucket group's move sends (section
9.2; T48, C18).

#### Scenario: An aborted write leaves nothing that exists
- **WHEN** a write creating `<gone>` is refused at commit, and a read then asks for every statement naming `<gone>`
- **THEN** the read returns nothing, whether or not the dictionary holds a record for `<gone>`

#### Scenario: A read at an older version is unaffected by a newer term
- **WHEN** `<late>` is first written at epoch 10 and a read at epoch 5 lists every statement of the default graph
- **THEN** the read returns no statement naming `<late>`

#### Scenario: A term to ID record is found by its name
- **WHEN** `<a>` is created, and a Scan range of space 1 and kind 1 names the XXH3-64 of `<a>`'s name bytes followed by
  those bytes
- **THEN** the entry's value is `<a>`'s system ID, and a range of kind 2 keyed by that ID answers 16 zero bytes followed
  by `<a>`'s name bytes

#### Scenario: A long literal's text is a record of kind 3
- **WHEN** a 40-byte string literal is written
- **THEN** a record of kind 3 keyed by the hash its term encoding holds holds the string, and writing the literal again
  writes the same key and value

#### Scenario: A re-created term keeps its old ID readable below the re-creation
- **WHEN** a round applied at version `v` re-creates an IRI held under an old ID, deleting its term to ID record of the
  old ID and writing one of a new ID
- **THEN** a term to ID read of the IRI at `v` or above answers the new ID, one below `v` the old ID, and the new ID's ID
  to term record names the old ID as the one it replaced

#### Scenario: A foreign term's copy is the replica's own
- **WHEN** partition 1 copies the term to ID and ID to term records of `<b>`, homed in partition 2
- **THEN** both copies lie in partition 1's families of bucket group `ABSENT`, and a move of any of partition 1's bucket
  groups sends neither

#### Scenario: A recreated IRI names its new instance and keeps its old one readable
- **WHEN** an aviation `Engine` is deleted and an `Engine` with the same `serial` is created
- **THEN** the IRI's term to ID record names the new ID from the re-creation's version, the old ID's ID to term record
  still names the IRI, and the new ID's ID to term record names the old ID

## ADDED Requirements

### Requirement: Handle IRIs are held both ways
A keyless instance's handle IRI SHALL be minted by its creating partition into its type's IRI space,
`{base}{Type}/{bucket}-{random}`: its random bits drawn as a blank node's are, and drawn again when the dictionary holds
the IRI; its bucket drawn so for an entity, and the anchor's for an unkeyed relation instance. The dictionary SHALL hold the IRI as the instance's name, mapping it to the ID and back; a result SHALL name the
instance by it as an IRI, never as a blank node; and a request naming it SHALL be accepted when the dictionary of the
partition owning the bucket it states holds it, and refused at the write naming it, with code 7 and `detail` the
term's index, when it does not.

#### Scenario: A handle is presented as an IRI
- **WHEN** a `Supply` instance is created from a blank node and its triples are read
- **THEN** every row names it by `https://example.org/aviation#Supply/` and its handle, a term of kind IRI

#### Scenario: A handle the dictionary does not hold is refused
- **WHEN** a Write names a handle IRI of `Supply`'s space whose bucket lies in the tenant's run and which no instance
  holds
- **THEN** the Write is refused with code 7, `detail` the term's index, before any commit

### Requirement: A governed tenant's declared predicates resolve from the catalog version
In a governed tenant a predicate IRI the catalog version in effect declares SHALL resolve to its ID, and the ID to the
IRI, from the version's Predicate entries and predicate index (system-identity, "A declared predicate's ID is
allocated with its catalog version"), in every partition and compute process alike: the dictionary SHALL hold no entry
for it, no partition SHALL register it at a home, and resolving it SHALL take no lock and read no store. A predicate
the version does not declare, which only an `@open` schema admits, and every predicate of an ungoverned tenant SHALL
resolve through the dictionary as before.

#### Scenario: A declared predicate is resolved without its home
- **WHEN** Gen5 is in effect over three partitions, and partition 1 first writes a statement whose predicate's IRI
  lies in a bucket partition 2 owns
- **THEN** partition 1 takes the predicate's ID from the version, sends no registration, and neither partition's
  dictionary holds an entry for the predicate

#### Scenario: A read names a declared predicate from the version
- **WHEN** compute answers a read of a Gen5 instance's triples
- **THEN** each declared predicate's IRI in the rows is the one the version's Predicate entry holds, and no dictionary
  entry is read for it
