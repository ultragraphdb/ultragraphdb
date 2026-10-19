## ADDED Requirements

### Requirement: The bulk loader builds one store per group
The bulk loader SHALL map, shuffle and reduce its input into one fjall store per group, placing reserved
predicates in group 1 and every predicate in exactly one store, with one write timestamp leased from Zero.

#### Scenario: K reduce shards make K stores
- **WHEN** the bulk loader runs with 3 reduce shards
- **THEN** it writes 3 stores, each predicate in exactly one, and the reserved predicates in the first

#### Scenario: Bulk and live loading agree
- **WHEN** the same input is bulk-loaded into one cluster and live-loaded into another
- **THEN** both answer the test queries alike

### Requirement: Import into a running cluster
A bulk-loaded store SHALL be imported into a running cluster's groups by streaming it to each group's replicas
while the cluster drains, and an import that fails SHALL drop the partial data.

#### Scenario: An imported cluster serves the data
- **WHEN** bulk-loaded stores are imported into a running two-group cluster
- **THEN** queries return the loaded data and the cluster leaves draining mode

#### Scenario: A failed import leaves no partial data
- **WHEN** an import stream breaks part way
- **THEN** the imported predicates are dropped and the cluster leaves draining mode
