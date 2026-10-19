## ADDED Requirements

### Requirement: The live loader commits every N-Quad
The live loader SHALL read RDF or JSON, plain or gzipped, send it in one-request batches (1,000 N-Quads, 10 in
flight by default), hold back a batch whose conflict keys overlap one in flight, and retry an aborted or failed
batch with backoff until it commits.

#### Scenario: Every N-Quad is loaded despite aborts
- **WHEN** the live loader loads a file while other clients write the same predicates
- **THEN** the count of loaded N-Quads equals the input's

#### Scenario: A gzipped file loads like the plain one
- **WHEN** the same data is loaded from a plain and a gzipped file into two empty clusters
- **THEN** both clusters answer the test queries alike

### Requirement: External IDs map to stable uids
The live loader SHALL keep input uids by default and lease past the highest, map blank nodes and external IDs
to uids, and, given an xidmap directory, map the same external ID to the same uid across runs.

#### Scenario: An xidmap keeps uids across runs
- **WHEN** a file is loaded twice with the same xidmap directory
- **THEN** each external ID maps to the same uid both times
