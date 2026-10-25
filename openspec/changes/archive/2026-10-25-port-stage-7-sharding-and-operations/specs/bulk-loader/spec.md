## ADDED Requirements

### Requirement: The bulk loader builds one store per group
The bulk loader SHALL map, shuffle and reduce its input into one fjall store per reduce shard, with at least as many map
shards as reduce shards, placing reserved predicates in the first and every predicate in exactly one, with one write
timestamp leased from Zero.

#### Scenario: K reduce shards make K stores
- **WHEN** the bulk loader runs with 3 map shards and 3 reduce shards
- **THEN** it writes 3 stores, each predicate in exactly one, and the reserved predicates in the first

#### Scenario: Bulk and live loading agree
- **WHEN** the same input is bulk-loaded into one cluster and live-loaded into another
- **THEN** both answer the test queries alike

### Requirement: Import into a running cluster
A bulk-loaded output with one store per cluster group SHALL replace each group's store through
`UpdateExtSnapshotStreamingState` and `StreamExtSnapshot`, streaming it to the group's replicas while the cluster
drains; an import that fails SHALL delete all data in every group, as Dgraph's does.

#### Scenario: An imported cluster serves the data
- **WHEN** bulk-loaded stores are imported into a running two-group cluster
- **THEN** queries return the loaded data and the cluster leaves draining mode

#### Scenario: A failed import leaves no data
- **WHEN** an import stream breaks part way
- **THEN** every group holds no data and the cluster leaves draining mode

### Requirement: The bulk loader leases namespaces from the namespace counter
The bulk loader SHALL lease the namespace identifiers it writes from Zero's namespace counter. Dgraph's bulk loader
leases them with `AllocateTimestamps` (ledger 28).

#### Scenario: A bulk load of three namespaces leases namespace identifiers
- **WHEN** a bulk load writes namespaces 0, 1 and 2
- **THEN** Zero's namespace counter is at least 2 afterwards and its timestamp counter did not lease them
