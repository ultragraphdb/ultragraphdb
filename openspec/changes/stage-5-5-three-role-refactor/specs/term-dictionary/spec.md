## REMOVED Requirements

### Requirement: IRIs and blank nodes get allocated UIDs
**Reason**: Replaced by 128-bit system IDs whose bucket is computed from the IRI and whose sequence comes from a block
the metadata group leases (system-identity).
**Migration**: "IRIs and blank nodes get system IDs" states the allocation; stores written before this stage are not
readable.

### Requirement: Concurrent creation of one IRI yields one UID
**Reason**: An IRI's ID is got or created at its home, so concurrent creators share one ID and none aborts (P24).
**Migration**: "An IRI's ID is got or created at its home".

## MODIFIED Requirements

### Requirement: The dictionary is a keyspace without versions
The dictionary SHALL map term to system ID, system ID to term and literal hash to string in a keyspace without versions,
read by point lookups. Its keys SHALL be its keyspace byte, the tenant's 8 big-endian bytes, a kind byte and then:
for an IRI or a reifier's name, the term's 128-bit hash (SHA-256 truncated to 16 bytes), its value the system ID, a
`Resource` ID for an IRI and a `Statement` ID for a reifier's name; for an ID, its 16 bytes, its value the term's text;
for a literal, its 128-bit hash, its value the string. It SHALL be naming state only: whether an entity exists, and
which types it has, SHALL be read from committed data, never from a dictionary entry, so an entry created for a write
that then aborted names nothing that exists. A literal's entry SHALL be written blind: equal strings write equal keys
and values. An ID's ID-to-term entry SHALL never change once written, since IDs are never reused, so any partition holds
a copy of another partition's ID-to-IRI entry without invalidation. Entries SHALL not be removed in this stage, so an
IRI-to-ID entry also never changes here; the entity delete of stage 6 and the reclamation of stage 8 change that
direction.

#### Scenario: An aborted write leaves nothing that exists
- **WHEN** a write creating `<gone>` is refused at commit, and a read then asks for every statement naming `<gone>`
- **THEN** the read returns nothing, whether or not the dictionary holds an entry for `<gone>`

#### Scenario: A read at an older version is unaffected by a newer term
- **WHEN** `<late>` is first written at epoch 10 and a read at epoch 5 lists every statement of the default graph
- **THEN** the read returns no statement naming `<late>`

### Requirement: A missing dictionary entry is corruption
A stored system ID or literal hash with no entry in its partition's dictionary SHALL be refused on read with an error
naming the key that holds it, as a corrupted value is. A dictionary range finding nothing is an ordinary empty answer at
storage, so compute SHALL make the check on a read through it and answer the read with code 17 (Unavailable), the message
naming the key holding the ID or hash, the connection staying open. An entity another partition owns that a partition's entries
reference SHALL have its copied entries in the partition's own dictionary ("A reference to another partition's entity
registers at its home"), so the rule holds for it locally.

#### Scenario: A dangling hash is reported
- **WHEN** a list names a literal hash whose dictionary entry is missing and the list is read
- **THEN** the read fails with an error naming the list's key, and nothing panics

#### Scenario: A foreign reference resolves locally
- **WHEN** partition 1 holds `<a> <knows> <b>`, `<b>` homed in partition 2, and reads `<a>`'s `knows` while partition 2
  is unreachable
- **THEN** the read names `<b>` by its IRI from partition 1's dictionary

#### Scenario: A dangling ID read through compute answers code 17
- **WHEN** a list names a system ID its partition's dictionary answers no entry for, and a client's read through compute
  reaches it
- **THEN** the read is refused with code 17, the message naming the list's key, and the client's connection stays open

## ADDED Requirements

### Requirement: IRIs and blank nodes get system IDs
Each IRI and each skolemized blank node, other than a reifier's skolem IRI, SHALL map, per tenant, to one system ID
of the `Resource` type, never reused: an IRI's bucket computed from the tenant and the IRI's canonical encoding, a
blank node's the one its skolem IRI states, and its sequence drawn from a block its home holds. The same IRI SHALL
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
- **WHEN** `<a> <knows> <b>` gains the property `<since> 2020` and so receives a statement ID
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
- **WHEN** a new IRI receives a system ID
- **THEN** its type prefix is 1 and its bucket equals the bucket computed from its tenant and canonical encoding

#### Scenario: A blank node's skolem IRI names its bucket and nothing else
- **WHEN** partition 1, owning the first 21,846 buckets of tenant 0's run, mints a blank node with its random source
  seeded to a recorded value
- **THEN** the skolem IRI is the tenant's skolem base, ending `/tenant/0/`, `.well-known/genid/`, 6 hexadecimal digits naming one of those
  buckets, `-` and 26 hexadecimal digits; the node's ID carries that bucket; and the same seed mints the same IRI

### Requirement: An IRI's ID is got or created at its home
An IRI's ID SHALL be got or created at its home while the writing transaction executes: the home SHALL return the ID its
dictionary holds for the IRI, or create the entry and return the new ID, and every writer of one IRI SHALL receive the
same ID. Creating an IRI's ID SHALL never abort a transaction. An entry the home creates for its own transaction SHALL
be durable no later than the round committing that transaction, and an entry it creates for another partition's
registration SHALL be durable before its reply ("A reference to another partition's entity registers at its home").

#### Scenario: Two creators share one ID
- **WHEN** two transactions in `<new>`'s home read before either commits, both write `<new> <p> "x"`, not held, and both
  send their commit
- **THEN** both receive one system ID for `<new>` and neither aborts for creating it; the two still conflict on the
  statement (transactions, "What a transaction's reads record and what conflicts with them"), so exactly one commits,
  and `<new>` has one system ID

#### Scenario: An aborted creator leaves the IRI usable
- **WHEN** a transaction creates `<new>` and aborts, and a later transaction writes `<new>`
- **THEN** the later transaction commits, and `<new>` has one system ID

### Requirement: A reference to another partition's entity registers at its home
Registration SHALL be per entity and referring bucket group: before a partition first commits a reference to an entity
`o` it does not own from a subject of bucket group `g`, it SHALL send `o`'s home a registration carrying the IRI's text
for an IRI it holds no ID for, or `o`'s ID, the group `g`, and the generation the partition last saw for `(o, g)`, 0
when it has seen none. The home SHALL get or create `o`'s ID, add or renew `g` in `o`'s referrer set (partitions, "An
entity's referrer set names where its inbound edges may be"), make both durable in its log (its durability ticket; the
VSR commit in stage 8), and only then reply with the ID, the member's generation and the latest close delivered to the
home. A registration SHALL be idempotent: a repeat SHALL return the same ID and change nothing. Registrations SHALL be
issued while the transaction executes and batched per home's role address per write request. The registering transaction's commit
SHALL write the registered mark of `(o, g)` with the generation the reply named (quad-storage, "The quad keyspaces"),
and, at the partition's first reference to `o` from any group, `o`'s dictionary entries in both directions into the
partition's own dictionary, so the partition names `o` and finds its ID without asking the home, after a restart too. A
later reference to `o` from group `g` SHALL find the mark and send no registration; a first reference from another group
of the same partition SHALL register that group. Concurrent registrants SHALL all receive the same ID, and none SHALL
abort for the race. A home SHALL answer outcome 7 for a skolem IRI its dictionary does not hold, which ends a write's
transaction with code 7 and reads as no entry for a Scan (quad-reads), and outcome 17 for a store that failed under the
registration, which ends the registrant's transaction with code 17 in either case, so a failed store never reads as a
skolem IRI nobody holds. A predicate or graph IRI another partition owns SHALL be got or created at its home before the
partition first uses it, by a registration that adds no referrer-set member, since nothing reads predicates or graphs
back through a referrer set, and its entries in both directions SHALL be copied into the partition's dictionary;
referrer sets concern IRIs and blank nodes in the object position only. A partition MAY cache other partitions'
IRI-to-ID entries in memory without invalidation: in this stage no entity is deleted, since deletes are by pattern, and
IDs are never reused, so a cached entry stays valid. The home's "deleted" answer and a delete's fence, which catch a
stale entry, arrive with the entity delete of stage 6.

#### Scenario: A first reference registers and waits
- **WHEN** a transaction in partition 1 writes `<a> <knows> <b>`, `<b>` homed in partition 2 and new
- **THEN** partition 2 creates `<b>`'s ID and adds `<a>`'s bucket group to `R(b)` durably before it replies, and
  partition 1 commits only after the reply, writing the mark of `(<b>, <a>'s group)` and `<b>`'s dictionary entries with
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
- **THEN** partition 1 gets `<knows>`'s ID from partition 2 before the commit and copies its entries in both directions,
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
