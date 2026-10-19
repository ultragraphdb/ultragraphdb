## ADDED Requirements

### Requirement: Alpha and Zero are separate processes
`ultragraph-zero` and `ultragraph-alpha` SHALL run as separate processes, the Alpha reaching Zero over gRPC for
timestamps, leases and membership, as Dgraph's Alpha reaches Zero.

#### Scenario: An Alpha starts against a running Zero
- **WHEN** a Zero starts, then an Alpha naming its address
- **THEN** the Alpha joins group 1, serves requests, and both report healthy

#### Scenario: An Alpha waits for its Zero
- **WHEN** an Alpha starts before its Zero
- **THEN** it reports unhealthy, retries, and serves once the Zero is up

### Requirement: Health and state
`/health` SHALL report each process's status and version, and `/state` SHALL report the membership, the group's
predicates and Zero's leases, as Dgraph's do.

#### Scenario: State lists the group's predicates
- **WHEN** data is written to `name` and `/state` is read from Zero
- **THEN** group 1 lists `name` among its tablets

### Requirement: Draining and shutdown
In draining mode the Alpha SHALL refuse new requests with `UNAVAILABLE`; a shutdown SHALL finish the requests in
flight and stop.

#### Scenario: Draining refuses new requests
- **WHEN** draining is switched on and a query arrives
- **THEN** it is refused with `UNAVAILABLE`
