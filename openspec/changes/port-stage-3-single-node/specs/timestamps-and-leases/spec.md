## ADDED Requirements

### Requirement: Zero issues timestamps in order
Zero SHALL issue start and commit timestamps that strictly increase, and SHALL report a timestamp as done only
when every timestamp at or below it is decided or applied.

#### Scenario: Timestamps increase
- **WHEN** 10,000 timestamps are requested concurrently
- **THEN** all are distinct and each caller's sequence increases

#### Scenario: A read waits for the commits below it
- **WHEN** a read timestamp is issued while a lower commit timestamp is being applied
- **THEN** the read returns only after that commit is applied

### Requirement: Read-only timestamps are reused
A read-only request SHALL receive the previous read-only timestamp when no write timestamp was issued since.

#### Scenario: Two reads with no write between share a timestamp
- **WHEN** two read-only queries run with no write between them
- **THEN** both report the same start timestamp

#### Scenario: best_effort reads skip Zero
- **WHEN** a read-only best-effort query runs
- **THEN** it reads at the Alpha's applied timestamp without asking Zero

### Requirement: Leases
Zero SHALL lease UIDs, namespace IDs and timestamps in blocks, never issue one twice, and persist the high-water
mark of each before handing out IDs under it. `AllocateIDs` SHALL lease the kind of ID it names.

#### Scenario: AllocateIDs leases the named kind
- **WHEN** the superadmin calls `AllocateIDs` for 10 IDs of each lease type
- **THEN** the UID, namespace and timestamp ranges come from their own counters, and a later timestamp is above the
  leased timestamp range

#### Scenario: Leases survive a restart
- **WHEN** Zero leases UIDs, restarts and leases again
- **THEN** no UID is issued twice

#### Scenario: AllocateIDs requires the superadmin
- **WHEN** a guardian of namespace 2 calls `AllocateIDs`
- **THEN** it is refused with `PERMISSION_DENIED`
