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
its bucket computed from the namespace and the IRI's canonical encoding, its sequence drawn from a block its partition
holds. The same IRI SHALL always map to the same ID, and its dictionary entries SHALL be held in the partition owning
its bucket.

#### Scenario: An IRI keeps its ID
- **WHEN** `<a>` is written in two separate commits
- **THEN** both name the same system ID

#### Scenario: An IRI's bucket is its own
- **WHEN** a new IRI receives a system ID
- **THEN** its type prefix is 1 and its bucket equals the bucket computed from its namespace and canonical encoding

### Requirement: Concurrent creation of one IRI yields one ID
Two writes that create the same new IRI concurrently SHALL end with one ID for it, decided in the partition owning the
IRI's bucket. The write committing second SHALL be refused with `Transaction has been aborted. Please retry`, writing
nothing, and its retry SHALL find the first write's ID.

#### Scenario: The second creator retries
- **WHEN** two writes read before either commits, both create `<new>`, and both commit
- **THEN** the first commits, the second fails with `Transaction has been aborted. Please retry`, and `<new>` has one
  ID
