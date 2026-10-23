## ADDED Requirements

### Requirement: The bulk loader leases namespaces from the namespace counter
The bulk loader SHALL lease the namespace identifiers it writes from Zero's namespace counter. Dgraph's bulk loader
leases them with `AllocateTimestamps` (ledger 28).

#### Scenario: A bulk load of three namespaces leases namespace identifiers
- **WHEN** a bulk load writes namespaces 0, 1 and 2
- **THEN** Zero's namespace counter is at least 2 afterwards and its timestamp counter did not lease them
