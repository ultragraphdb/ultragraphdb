## ADDED Requirements

### Requirement: The simulated store models the keyspace without versions
The simulated store SHALL hold the keyspace without versions under the same fault stream and shadow model as its
versioned keyspaces, and the shadow model SHALL check after every step and every recovery that each batch's versionless
entries are visible exactly when its versioned entries are.

#### Scenario: A power loss keeps a batch's two halves together
- **WHEN** batches write versioned and versionless keys together and a power loss is injected
- **THEN** after recovery every surviving batch's versionless entries are present, and every lost batch's are absent

#### Scenario: The dictionary workload passes on the simulator
- **WHEN** a seeded workload creates terms and writes quads naming them under every fault class
- **THEN** every stored UID and literal hash has its dictionary entry after every step and every recovery
