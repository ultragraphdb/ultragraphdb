## REMOVED Requirements

### Requirement: The schema persists in the store
**Reason**: The catalog moves to the metadata group, where schema versions take effect at epoch boundaries (D4).
**Migration**: "The schema persists in the metadata group"; partitions read the catalog version in effect from the
metadata group.

## MODIFIED Requirements

### Requirement: Writes to an unknown predicate create its schema
A write to a predicate the catalog version in effect does not declare SHALL be validated with the default entry, kind
either, multi-valued, no datatype constraint and no index, and SHALL ask the metadata group to record that entry as a
default-entry record keyed by (tenant, predicate), a command that finds the record when it already exists and creates
no catalog version; a write that fails SHALL ask for none. No write SHALL wait for the command; on recovery a partition
SHALL send it again for every undeclared predicate its replayed rounds wrote, which the command's finding an existing
record makes harmless; and concurrent first writes of one predicate SHALL record the same default entry, so neither
depends on the other (commit-rounds, "Catalog changes, drops and index builds are ordered in rounds").

#### Scenario: A first write records the predicate
- **WHEN** `<a> <nickname> "Al"` is written and `nickname` is not declared
- **THEN** once the command is answered the metadata group holds the default-entry record of `nickname`, and the
  catalog reads `nickname` with the default entry

#### Scenario: Recovery asks again for a default entry
- **WHEN** a commit first writes the undeclared `alias` and the process is killed before its default-entry command is
  sent, and the partition recovers
- **THEN** replay sends the command for `alias`, and once it is answered the metadata group holds its record

#### Scenario: A failed write records no predicate
- **WHEN** a write naming `<alias>` fails before it commits and `alias` was not declared
- **THEN** the catalog does not hold `alias`

#### Scenario: Concurrent first writes both commit
- **WHEN** two writes read before either commits, the first writes `<a> <score> 12` and the second `<b> <score> "high"`,
  and `score` was not declared
- **THEN** both commit, and the catalog holds `score` with the default entry

### Requirement: A catalog entry that does not read back is corruption
A catalog version the metadata group holds that does not decode SHALL refuse the load of its tenant's catalog, by
every role that loads it, with the corruption error naming the version, skipping nothing: every version is written from
declarations through the catalog's encoding, so one that does not read back is a fault of the store or of the code, and
a declaration skipped would let writes go on under the default declaration.

#### Scenario: A catalog version that does not decode refuses the load
- **WHEN** a tenant's catalog is loaded from a metadata group holding a version that does not decode
- **THEN** the load fails with a corruption error naming the version, and nothing panics

## ADDED Requirements

### Requirement: The schema persists in the metadata group
Each tenant's catalog SHALL be committed to the metadata group as numbered versions and loaded from it at startup by
every role that validates against it, in place of catalog keys in each partition's store; a partition SHALL hold the
versions in effect at its open epochs, and the versions its untrimmed log names, loaded before it replays, and no other
copy of the catalog.

#### Scenario: The catalog survives a restart
- **WHEN** declarations are made and the frontend and storage are stopped and started
- **THEN** the loaded catalog equals the catalog before stopping

#### Scenario: A removed declaration stays removed after a restart
- **WHEN** a predicate's declaration is removed and the roles are restarted
- **THEN** the loaded catalog does not declare the predicate

#### Scenario: No catalog key is written to a partition
- **WHEN** a predicate is declared
- **THEN** no partition's store holds a catalog key for it
