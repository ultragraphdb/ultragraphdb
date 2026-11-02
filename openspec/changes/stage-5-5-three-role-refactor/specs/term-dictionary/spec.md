## REMOVED Requirements

### Requirement: IRIs and blank nodes get allocated UIDs
**Reason**: Replaced by 128-bit system IDs whose bucket is computed from the IRI and whose sequence comes from a block
the metadata group leases (system-identity).
**Migration**: "IRIs and blank nodes get system IDs" states the allocation; stores written before this stage are not
readable.

### Requirement: Concurrent creation of one IRI yields one UID
**Reason**: Restated for system IDs, decided in the partition owning the IRI's bucket.
**Migration**: "Concurrent creation of one IRI yields one ID".

## ADDED Requirements

### Requirement: IRIs and blank nodes get system IDs
Each IRI and each skolemized blank node SHALL map, per namespace, to one system ID of the `Resource` type, never reused:
an IRI's bucket computed from the namespace and the IRI's canonical encoding, a blank node's the one its skolem IRI
states, and its sequence drawn from a block its partition holds. The same IRI SHALL always map to the same ID, and its
dictionary entries SHALL be held in the partition owning its bucket.

A new blank node's skolem IRI SHALL be the namespace's skolem base followed by a bucket and 104 random bits, as
`…/.well-known/genid/{bucket}-{random}`, the bucket in 6 and the random bits in 26 lowercase hexadecimal digits. The
partition minting it SHALL draw both from its injected random source: the bucket from those it owns in the namespace's
run, the random bits uniformly. A skolem IRI that names an IRI the dictionary already holds SHALL be drawn again. The
skolem IRI SHALL hold no part of the system ID beyond its bucket. A transaction whose first write names only new blank
nodes SHALL bind to a partition the frontend chooses in turn.

#### Scenario: An IRI keeps its ID
- **WHEN** `<a>` is written in two separate commits
- **THEN** both name the same system ID

#### Scenario: An IRI's bucket is its own
- **WHEN** a new IRI receives a system ID
- **THEN** its type prefix is 1 and its bucket equals the bucket computed from its namespace and canonical encoding

#### Scenario: A blank node's skolem IRI names its bucket and nothing else
- **WHEN** partition 1, owning the first 21,846 buckets of namespace 0's run, mints a blank node with its random source
  seeded to a recorded value
- **THEN** the skolem IRI is the namespace's skolem base, `.well-known/genid/`, 6 hexadecimal digits naming one of those
  buckets, `-` and 26 hexadecimal digits; the node's ID carries that bucket; and the same seed mints the same IRI

### Requirement: Concurrent creation of one IRI yields one ID
Two writes that create the same new IRI concurrently SHALL end with one ID for it, decided in the partition owning the
IRI's bucket. The write committing second SHALL be refused with `Transaction has been aborted. Please retry`, writing
nothing, and its retry SHALL find the first write's ID.

#### Scenario: The second creator retries
- **WHEN** two writes read before either commits, both create `<new>`, and both commit
- **THEN** the first commits, the second fails with `Transaction has been aborted. Please retry`, and `<new>` has one
  ID
