## ADDED Requirements

### Requirement: The dictionary is kept in record runs of kinds 1 to 3
The dictionary SHALL be three kinds of record (`STORAGE.md` section 9.3), read by Scan ranges of space 1 (`PROTOCOL.md`
section 13.13): term to ID (kind 1), its key the XXH3-64 of the term's name bytes as a u64 followed by the name bytes,
a name's bytes being its kind, 1 an IRI, 2 a blank node's skolem IRI, 3 a reifier's name or 4 a handle IRI, and then
its UTF-8 text, its value the system ID; ID to term (kind 2), its key the system ID, its value the ID the term's
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

## MODIFIED Requirements

### Requirement: A missing dictionary entry is corruption
A stored system ID or literal hash with no record in its partition's dictionary SHALL be refused on read with an error
naming the row or record that holds it, as a corrupted value is. A dictionary range finding nothing is an ordinary
empty answer at storage, so compute SHALL make the check on a read through it and answer the read with code 17
(Unavailable), the message naming the row holding the ID or hash, the connection staying open (`PROTOCOL.md` section
13.12). An entity another partition owns that a partition's entries reference SHALL have its copied records in the
partition's own dictionary ("A reference to another partition's entity registers at its home"), so the rule holds for
it locally.

#### Scenario: A dangling hash is reported
- **WHEN** a row names a literal hash whose dictionary record is missing and the row is read
- **THEN** the read fails with an error naming the row, and nothing panics

#### Scenario: A foreign reference resolves locally
- **WHEN** partition 1 holds `<a> <knows> <b>`, `<b>` homed in partition 2, and reads `<a>`'s `knows` while partition 2
  is unreachable
- **THEN** the read names `<b>` by its IRI from partition 1's dictionary

#### Scenario: A dangling ID read through compute answers code 17
- **WHEN** a row names a system ID its partition's dictionary answers no record for, and a client's read through
  compute reaches it
- **THEN** the read is refused with code 17, the message naming the row, and the client's connection stays open

### Requirement: A reference to another partition's entity registers at its home
Registration SHALL be per entity and referring bucket group: before a partition first commits a reference to an entity
`o` it does not own from a subject of bucket group `g`, it SHALL send `o`'s home a registration carrying the IRI's text
for an IRI it holds no ID for, or `o`'s ID, the group `g`, and the generation the partition last saw for `(o, g)`, 0
when it has seen none. The home SHALL get or create `o`'s ID, add or renew `g` in `o`'s referrer set (partitions, "An
entity's referrer set names where its inbound edges may be"), make both durable in its log (its durability ticket; the
VSR commit in stage 8), and only then reply with the ID, the member's generation and the latest close delivered to the
home. A registration SHALL be idempotent: a repeat SHALL return the same ID and change nothing. Registrations SHALL be
issued while the transaction executes and batched per home's role address per write request. The registering
transaction's commit SHALL write the registered mark of `(o, g)` (`STORAGE.md` section 9.3, kind 6) with the generation
the reply named (quad-storage, "Quads are kept in tiles' families"), and, at the partition's first reference to `o`
from any group, `o`'s term to ID and ID to term records into the partition's own dictionary, in its family of bucket
group `ABSENT` ("The dictionary is kept in record runs of kinds 1 to 3"), so the partition names `o` and finds its ID
without asking the home, after a restart too. A later reference to `o` from group `g` SHALL find the mark and send no
registration; a first reference from another group of the same partition SHALL register that group. Concurrent
registrants SHALL all receive the same ID, and none SHALL abort for the race. A home SHALL answer outcome 7 for a skolem
IRI its dictionary does not hold, which ends a write's transaction with code 7 and reads as no entry for a Scan
(quad-reads), and outcome 17 for a store that failed under the registration, which ends the registrant's transaction
with code 17 in either case, so a failed store never reads as a skolem IRI nobody holds, and outcome 26 for a bucket it
does not own, which ends the registrant's transaction with code 26 in either case, `detail` 0, so the client runs it
again on a new Begin. A predicate or graph IRI another partition owns SHALL be got or created at its home before the
partition first uses it, by a registration that adds no referrer-set member, since nothing reads predicates or graphs
back through a referrer set, and its records in both directions SHALL be copied into the partition's dictionary;
referrer sets concern IRIs and blank nodes in the object position only. A partition MAY cache other partitions'
IRI-to-ID records in memory without invalidation: in this stage no entity is deleted, since deletes are by pattern, and
IDs are never reused, so a cached record stays valid. The home's "deleted" answer and a delete's fence, which catch a
stale record, arrive with the entity delete of stage 6.6.

#### Scenario: A first reference registers and waits
- **WHEN** a transaction in partition 1 writes `<a> <knows> <b>`, `<b>` homed in partition 2 and new
- **THEN** partition 2 creates `<b>`'s ID and adds `<a>`'s bucket group to `R(b)` durably before it replies, and
  partition 1 commits only after the reply, writing the mark of `(<b>, <a>'s group)` and `<b>`'s dictionary records with
  `<a>`'s statement

#### Scenario: A marked reference sends no registration
- **WHEN** partition 1 has committed a reference to `<b>` from a subject of bucket group 7, and a later transaction
  writes another edge to `<b>` from a subject of group 7
- **THEN** it finds `<b>`'s ID and the mark in its own store, sends no registration, and commits without waiting for
  partition 2, after a restart too

#### Scenario: A second group of the same partition registers
- **WHEN** partition 1 holds the mark of `(<b>, 7)`, and a transaction writes an edge to `<b>` from a subject of
  partition 1's bucket group 9
- **THEN** it registers `(<b>, 9)` at partition 2, `R(b)` names groups 7 and 9, and partition 1 then holds both marks

#### Scenario: A foreign predicate is got without joining a referrer set
- **WHEN** partition 1 first writes a statement whose predicate `<knows>` is homed in partition 2
- **THEN** partition 1 gets `<knows>`'s ID from partition 2 before the commit and copies its records in both directions,
  and no referrer set gains a member for `<knows>`

#### Scenario: Registrations to one node are batched
- **WHEN** a transaction in partition 1 writes edges to 50 new IRIs in one write request, all homed in partitions of
  another storage process
- **THEN** partition 1 sends that process one batch of 50 registrations for the request

#### Scenario: Concurrent registrants share one ID
- **WHEN** partitions 1 and 3 each register a reference to the new IRI `<b>` at its home, both registrations reaching
  the home before it replies to either
- **THEN** both replies name the same ID, both partitions' transactions commit, and `R(b)` names both groups

#### Scenario: A forged skolem IRI in a write is refused at its home
- **WHEN** a transaction in partition 1 writes an edge to a skolem IRI whose bucket partition 2 owns, and partition 2's
  dictionary does not hold it
- **THEN** the registration's outcome is 7, and the Write is refused with code 7, ending the transaction

#### Scenario: A home's failed store never reads as absent
- **WHEN** a transaction in partition 1 names, in a Write and in a Scan, a skolem IRI homed in partition 2, whose store
  fails under the registration
- **THEN** the registration's outcome is 17, each request is refused with code 17, and the transaction ends

## REMOVED Requirements

### Requirement: The dictionary is a keyspace without versions
**Reason**: The keyspace without versions is removed with the KV store. The dictionary is three record kinds of the
tile store, and its term to ID records are of class M and versioned, so a re-created term's old ID stays readable below
the re-creation (`STORAGE.md` sections 9.3, 14.3 and 15.3; T48, C16); its keys are the term's name bytes under an
XXH3-64, no longer a SHA-256 hash.
**Migration**: "The dictionary is kept in record runs of kinds 1 to 3". Stores written before this change are not
readable.
