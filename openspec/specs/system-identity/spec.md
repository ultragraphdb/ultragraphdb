# system-identity Specification

## Purpose
The 128-bit system ID naming every entity and statement inside the database: a type prefix, a sequence unique per type
and leased in blocks per partition, a bucket computed from the tenant and the key, and reserved bits, ordered so a
sorted list's deltas stay small and placed so an ID's bucket names its partition. Clients name entities by IRIs, blank
nodes by skolem IRIs and statements by their quads or reifiers, and no system ID leaves the database.

## Requirements
### Requirement: A system ID is 128 bits of type, sequence, bucket and reserved bits
Every entity and every statement SHALL be named inside the database by a 128-bit system ID: from the most significant
bit, a 16-bit type prefix, a 64-bit sequence, a 24-bit bucket and 24 reserved bits, which SHALL be zero. The ID SHALL
be stored and compared as an unsigned 128-bit big-endian value, so IDs of one type sort together and, within a type,
by sequence then bucket: the bucket's random bits sit below the sequence, so the IDs of a sorted list differ mostly in
their sequences and their deltas stay small. An ID whose reserved bits are not zero, whose type prefix is 0 or whose sequence is 0 SHALL be
refused as invalid wherever it is decoded.

#### Scenario: The fields pack and unpack
- **WHEN** an ID is built from type prefix 3, bucket 0xAB_CDEF and sequence 42
- **THEN** its 16 big-endian bytes are `00 03`, `00 00 00 00 00 00 00 2A`, `AB CD EF` and `00 00 00`, and unpacking
  returns type 3, bucket 0xAB_CDEF and sequence 42

#### Scenario: Nonzero reserved bits are refused
- **WHEN** 16 bytes whose last byte is 0x01 are decoded as an ID
- **THEN** decoding fails with an invalid-ID error naming the reserved bits

#### Scenario: A zero type prefix or sequence is refused
- **WHEN** an ID with type prefix 0, and another with sequence 0, are decoded
- **THEN** each fails with an invalid-ID error naming the field

#### Scenario: IDs of one type sort together
- **WHEN** the IDs (type 2, bucket 9, sequence 1), (type 1, bucket 900, sequence 7) and (type 1, bucket 3, sequence 99)
  are sorted by their bytes
- **THEN** the order is (1, 900, 7), (1, 3, 99), (2, 9, 1): type first, then sequence

### Requirement: The bucket comes from the tenant and the key
An entity's bucket SHALL be fixed when the entity is created and SHALL be computed, never chosen: its top 8 bits SHALL
be the top 8 bits of XXH3-64 (seed 0) of the tenant as 8 big-endian bytes, and its low 16 bits SHALL be the top 16
bits of XXH3-64 (seed 0) of the declaring type's prefix as 2 big-endian bytes followed by the key's canonical encoding.
A statement SHALL take the bucket of its subject. A blank node, which has no key, SHALL take the bucket its skolem IRI
states (term-dictionary, "IRIs and blank nodes get system IDs"). A value of a predicate declared unique SHALL be claimed
in the bucket computed the same way with the predicate's system ID as its 16 big-endian bytes, followed by the
comparison form of the value's sortable encoding in a compiled layout (`STORAGE.md` section 5.2: a temporal value's
first 16 bytes, any other value's whole encoding) or its term encoding in the generic layout, in place of the type's
prefix and the key, so equal values of one predicate are claimed in one bucket whatever their offset and the partition
owning that bucket decides the claim (T48). The
same tenant, type and key SHALL always give the same bucket on every node and in every version that reads it.

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
  reference XXH3-64 of `epi.id`'s 16 ID bytes followed by the comparison form of `"x"`'s sortable encoding, its UTF-8
  byte, in a compiled layout, or `"x"`'s term encoding in the generic layout

#### Scenario: Two tenants separate one key
- **WHEN** the IRI `http://ex.org/a` is bucketed in tenants 0 and 1
- **THEN** the two buckets' low 16 bits are equal, and their top 8 bits are the reference values for each tenant

#### Scenario: Two tenants sharing a run keep one key apart
- **WHEN** tenants A and B, whose hashes share their top 8 bits, each write the IRI `http://ex.org/a`
- **THEN** the two buckets are equal, each tenant's dictionary holds its own entry under its own key, and each IRI has
  its own system ID

### Requirement: Until UGS keys exist, an IRI is the key of the Resource type
In this stage every IRI-named entity SHALL be of the built-in type `Resource`, whose key is the IRI, and every statement
SHALL be of the built-in type `Statement`; their prefixes SHALL be reserved before any declared type: prefix 0 invalid,
1 `Resource`, 2 `Statement`. The canonical encoding of an IRI key SHALL be its UTF-8 bytes as RDF 1.2 writes the IRI,
without surrounding angle brackets, prefixed by its length as a 4-byte big-endian count.

#### Scenario: The canonical encoding of an IRI
- **WHEN** the IRI `http://ex.org/a` is canonically encoded
- **THEN** the bytes are `00 00 00 0F` followed by the 15 UTF-8 bytes of `http://ex.org/a`

#### Scenario: The reserved prefixes
- **WHEN** a new IRI and a new statement receive IDs
- **THEN** the IRI's type prefix is 1 and the statement's is 2

### Requirement: Sequences are unique per type and leased in blocks per partition
A sequence SHALL be unique within its type across the cluster and SHALL never be reused. A partition SHALL draw
sequences from blocks the metadata group leases to it per (type, partition), each block a half-open range of at least
one sequence; a partition SHALL lease its next block before its current one runs out and SHALL allocate no sequence
outside a block it holds. A block leased and not used SHALL never be leased again.

#### Scenario: Two partitions never collide
- **WHEN** partitions 1 and 2 each lease a block of the `Resource` type and allocate every sequence in it
- **THEN** no sequence is allocated twice, and the two blocks do not overlap

#### Scenario: A restarted partition does not reuse a sequence
- **WHEN** a partition allocates sequences 1 to 10 of a block 1..1000, crashes, and restarts
- **THEN** it allocates no sequence it allocated before the crash, and resumes from a sequence above 10 or a new block

#### Scenario: A used-up block gives way to the next
- **WHEN** a partition holding the block 1000..2000 and the next block 5000..6000 allocates after sequence 1999
- **THEN** it allocates sequence 5000

#### Scenario: A sequence outside a held block is refused
- **WHEN** a partition holding only the block 1000..2000, with the metadata group unreachable, allocates after sequence
  1999
- **THEN** it refuses with an error naming the `Resource` type, and allocates nothing

### Requirement: System IDs never leave the database
No reply, result batch, error, cursor, resume key or IRI sent to a client SHALL contain a system ID. Entities SHALL be
named to clients by their IRIs, a blank node by its skolem IRI, and the node a commit-time operation found or created
likewise. A statement without a statement ID SHALL be named to clients by its quad, the triple term of its subject,
predicate and object with its graph, which is unique among statements without one; a statement with a statement ID,
because it carries properties or repeats a quad as a parallel edge, SHALL be named by its reifier, a skolem IRI minted
as a blank node's with the statement's bucket (term-dictionary, "IRIs and blank nodes get system IDs"). Neither SHALL be
named by its statement ID. A resume key SHALL be an opaque, checksummed token the server alone decodes, carrying the
read's last row by its terms in text form, the pattern's hash and what binds it to its transaction (quad-reads, "Resume
keys continue their own read"), never a store key.

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

