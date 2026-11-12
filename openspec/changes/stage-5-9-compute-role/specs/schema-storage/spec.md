## MODIFIED Requirements

### Requirement: The schema persists in the metadata group
Each tenant's catalog SHALL be committed to the metadata group as numbered versions and loaded from it at startup by
every role that validates against it, in place of catalog keys in each partition's store; a partition SHALL hold the
versions in effect at its open epochs, and the versions its untrimmed log names, loaded before it replays, and no other
copy of the catalog.

#### Scenario: The catalog survives a restart
- **WHEN** declarations are made and compute and storage are stopped and started
- **THEN** the loaded catalog equals the catalog before stopping

#### Scenario: A removed declaration stays removed after a restart
- **WHEN** a predicate's declaration is removed and the roles are restarted
- **THEN** the loaded catalog does not declare the predicate

#### Scenario: No catalog key is written to a partition
- **WHEN** a predicate is declared
- **THEN** no partition's store holds a catalog key for it
