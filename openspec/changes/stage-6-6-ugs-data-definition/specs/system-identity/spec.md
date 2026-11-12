## RENAMED Requirements

- FROM: `### Requirement: Until UGS keys exist, an IRI is the key of the Resource type`
- TO: `### Requirement: An IRI is the key of the Resource type`

## MODIFIED Requirements

### Requirement: An IRI is the key of the Resource type
Every IRI-named entity that is no instance of a declared type SHALL be of the built-in type `Resource`, whose key is
the IRI, in a tenant with no schema and where an `@open` schema admits it, and every statement that is no declared
relation's instance SHALL be of the built-in type `Statement`; their prefixes SHALL be reserved before any declared type: prefix 0 invalid,
1 `Resource`, 2 `Statement`. The canonical encoding of an IRI key SHALL be its UTF-8 bytes as RDF 1.2 writes the IRI,
without surrounding angle brackets, prefixed by its length as a 4-byte big-endian count: the encoding of a key of one
component (ugs-identity, "Keys have one canonical encoding"), so governed and ungoverned data hash alike.

#### Scenario: The canonical encoding of an IRI
- **WHEN** the IRI `http://ex.org/a` is canonically encoded
- **THEN** the bytes are `00 00 00 0F` followed by the 15 UTF-8 bytes of `http://ex.org/a`

#### Scenario: The reserved prefixes
- **WHEN** a new IRI and a new statement receive IDs in a tenant with no schema
- **THEN** the IRI's type prefix is 1 and the statement's is 2

### Requirement: The bucket comes from the tenant and the key
An entity's bucket SHALL be fixed when the entity is created and, for an entity named by a key or by its IRI, SHALL be
computed, never chosen: its top 8 bits SHALL
be the top 8 bits of XXH3-64 (seed 0) of the tenant as 8 big-endian bytes, and its low 16 bits SHALL be the top 16
bits of XXH3-64 (seed 0) of the declaring type's prefix as 2 big-endian bytes followed by the key's canonical encoding.
A statement SHALL take the bucket of its subject. A blank node, which has no key, SHALL take the bucket its skolem IRI
states (term-dictionary, "IRIs and blank nodes get system IDs"). A value of a predicate declared unique SHALL be claimed
in the bucket computed the same way with the predicate's system ID as its 16 big-endian bytes, followed by the value's
term encoding, in place of the type's prefix and the key, so the partition owning that bucket decides the claim. The
same tenant, type and key SHALL always give the same bucket on every node and in every version that reads it. A keyless instance of a declared type SHALL take the bucket its handle states, which its partition draws
from those it owns or, for an unkeyed relation instance, takes from its anchor. A `@key` attribute's value SHALL be
claimed in its instance's own bucket, of the key space's prefix and the canonical key; the value's bucket above is a
`@unique` attribute's and an ungoverned unique predicate's.

#### Scenario: The bucket matches the XXH3 reference
- **WHEN** the bucket of the IRI `http://ex.org/a` in tenant 0 is computed
- **THEN** its top 8 bits equal the top byte of the xxHash reference implementation's XXH3-64 of eight zero bytes, and
  its low 16 bits equal the top two bytes of the reference XXH3-64 of the `Resource` prefix's 2 bytes followed by the
  IRI's canonical encoding, both values committed as golden values with their source

#### Scenario: A statement shares its subject's bucket
- **WHEN** a statement with subject `http://ex.org/a` receives a system ID
- **THEN** the statement's bucket equals the bucket of `http://ex.org/a`

#### Scenario: A unique value's bucket is computed from its predicate and value
- **WHEN** the bucket of the value `"x"` of the unique predicate `epi.id` is computed in tenant 0
- **THEN** its top 8 bits equal the tenant's reference value, and its low 16 bits equal the top two bytes of the
  reference XXH3-64 of `epi.id`'s 16 ID bytes followed by `"x"`'s term encoding

#### Scenario: Two tenants separate one key
- **WHEN** the IRI `http://ex.org/a` is bucketed in tenants 0 and 1
- **THEN** the two buckets' low 16 bits are equal, and their top 8 bits are the reference values for each tenant

#### Scenario: Two tenants sharing a run keep one key apart
- **WHEN** tenants A and B, whose hashes share their top 8 bits, each write the IRI `http://ex.org/a`
- **THEN** the two buckets are equal, each tenant's dictionary holds its own entry under its own key, and each IRI has
  its own system ID

### Requirement: System IDs never leave the database
No reply, result batch, error, cursor, resume key or IRI sent to a client SHALL contain a system ID. Entities SHALL be
named to clients by their IRIs, a blank node by its skolem IRI, and the node a commit-time operation found or created
likewise. A statement without a statement ID SHALL be named to clients by its quad, the triple term of its subject,
predicate and object with its graph, which is unique among statements without one; a statement with a statement ID,
because it carries properties or repeats a quad as a parallel edge, SHALL be named by its reifier, a skolem IRI minted
as a blank node's with the statement's bucket (term-dictionary, "IRIs and blank nodes get system IDs"). Neither SHALL be
named by its statement ID. A resume key SHALL be an opaque, checksummed token the server alone decodes, carrying the
read's last row by its terms in text form, the pattern's hash and what binds it to its transaction (quad-reads, "Resume
keys continue their own read"), never a store key. In a governed tenant a statement SHALL hold a statement ID also where ugs-views, "A relation type is
edge-encoded or node-encoded by its shape", allocates one, and a client SHALL name it by its reifier's handle IRI.

#### Scenario: A result names an IRI by its text
- **WHEN** a client reads the subject of a statement whose subject is `http://ex.org/a`
- **THEN** the result batch names `http://ex.org/a` by its text, and its bytes contain no system ID

#### Scenario: A statement with a statement ID is named by its reifier
- **WHEN** a client reads a statement that carries a property and so holds a statement ID
- **THEN** the result names the statement by its quad and its reifier's skolem IRI, whose text holds the statement's
  bucket and no sequence, and its bytes contain no system ID

#### Scenario: A resume key carries terms, not keys
- **WHEN** a read stops at its limit after the row whose object is `http://ex.org/b`, and the server decodes the resume
  key it returned
- **THEN** the key holds the pattern's hash and the text `http://ex.org/b`, and no store key or system ID

#### Scenario: A tampered resume key is refused
- **WHEN** a client changes one byte of a resume key and continues its read with it
- **THEN** the read is refused with an error naming the resume key as malformed

## ADDED Requirements

### Requirement: An instance of a declared type carries its type's prefix
In a governed tenant an instance's system ID SHALL take the type prefix of its concrete type at creation and keep it
for life, its sequence drawn from the block its partition leases for that type. A keyed instance's bucket SHALL be
computed from its key space's prefix and its canonical key, a keyless entity's SHALL be the one its handle states, and
an unkeyed relation instance's its anchor participant's (ugs-identity, "Buckets and claims come from the key space").

#### Scenario: A declared instance's ID
- **WHEN** an aviation `Engine` with `serial` `EN-4471-B` is created
- **THEN** its ID's type prefix is 5, `Engine`'s, and its bucket is the one computed from prefix 3, `Asset`'s, and the
  canonical key

#### Scenario: A keyless entity's bucket is its handle's
- **WHEN** a partition creates a keyless entity and mints its handle
- **THEN** the entity's ID carries the bucket the handle's first six digits state

### Requirement: A declared predicate's ID is allocated with its catalog version
In a governed tenant each predicate a catalog version declares SHALL have the system ID of the `Resource` prefix 1,
its IRI's bucket computed as any IRI's, and a sequence of at least 2^63 the metadata group allocates with the version
(metadata-group, "The metadata group allocates type prefixes and predicate sequences"), held in the version's predicate
index and in no entry. A `Resource` sequence leased to a partition SHALL stay below 2^63, so no other ID equals a
declared predicate's, and a declared predicate's ID SHALL be the same in every partition and compute process.

#### Scenario: The aviation schema's predicates
- **WHEN** the aviation schema is installed into an empty tenant 0
- **THEN** each declared predicate's ID has type prefix 1, the bucket of its IRI in tenant 0, and a sequence from 2^63
  upward in the order of the version's Predicate entries

#### Scenario: A unique value's bucket follows the version
- **WHEN** an aviation `@unique` attribute's value is claimed on two storage processes started from one catalog version
- **THEN** both compute the claim's bucket from the same predicate ID, the one the version holds
