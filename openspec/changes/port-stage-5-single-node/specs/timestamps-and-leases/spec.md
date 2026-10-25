## ADDED Requirements

### Requirement: Zero issues timestamps in order
Zero SHALL issue start and commit timestamps that strictly increase, and SHALL report a timestamp as done only when
every timestamp at or below it is decided or applied.

#### Scenario: Timestamps increase
- **WHEN** 10,000 non-read-only timestamps are requested concurrently
- **THEN** all are distinct and each caller's sequence increases

#### Scenario: A read waits for the commits below it
- **WHEN** a read timestamp is issued while a lower commit timestamp is being applied
- **THEN** the read returns only after that commit is applied

### Requirement: Read-only timestamps are reused
A read-only request SHALL receive the previous read-only timestamp when no other timestamp was issued since; a
best-effort read SHALL read at the highest done watermark the Alpha has received, asking Zero only when it has none.

#### Scenario: Two reads with no other timestamp between share one
- **WHEN** two read-only queries run and no other timestamp is issued between them
- **THEN** both report the same start timestamp

#### Scenario: best_effort reads use the received watermark
- **WHEN** a read-only best-effort query runs on an Alpha that has received a done watermark
- **THEN** it reads at that watermark without asking Zero

### Requirement: Leases
Zero SHALL lease uids, namespace IDs and timestamps in blocks, persist each kind's high-water mark in its own store
before handing out IDs under it, and never issue one twice. It SHALL accept a bump lease, answering
`Nothing to be leased` when already past it, refuse a lease beyond the counter's limit with Dgraph's error, and limit uid
leases per namespace as Dgraph does. A mutation naming a uid above the uid lease SHALL be refused with
`Uid: [<uid>] cannot be greater than lease: [<lease>]`.

#### Scenario: Leases survive a restart
- **WHEN** Zero leases uids, restarts and leases again
- **THEN** no uid is issued twice

#### Scenario: A bump below the lease leases nothing
- **WHEN** a bump lease names a uid below the current uid lease
- **THEN** Zero answers `Nothing to be leased`

#### Scenario: A uid above the lease is refused
- **WHEN** a mutation sets `<0xffffffffff> <name> "x" .` above the current uid lease
- **THEN** it is refused with `Uid: [1099511627775] cannot be greater than lease:` and the lease

### Requirement: AllocateIDs leases the kind it names
`AllocateIDs` SHALL lease the kind of ID it names, and SHALL report timestamps it leases as done at lease time so no read
waits on them; Dgraph leases uids for every kind (`edgraph/zero.go`, `worker/mutation.go:608`). With ACL on it SHALL
require the superadmin; with ACL off it SHALL be open, as in Dgraph.

#### Scenario: AllocateIDs leases the named kind
- **WHEN** the superadmin calls `AllocateIDs` for 10 IDs of each lease type
- **THEN** the uid, namespace and timestamp ranges come from their own counters, and a later read does not wait on the
  leased timestamps

#### Scenario: AllocateIDs requires the superadmin under ACL
- **WHEN** ACL is on and a guardian of namespace 2 calls `AllocateIDs`
- **THEN** it is refused with `PERMISSION_DENIED`
