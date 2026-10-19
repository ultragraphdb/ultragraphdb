## ADDED Requirements

### Requirement: Backups write Dgraph's layout
A backup SHALL write one file per group, `dgraph.<time>/r<ts>-g<gid>.backup`, holding each posting list in full at the
backup's timestamp, and SHALL add an entry to `manifest.json` with its type, since and read timestamps, backup ID,
number, groups and predicates, and drop operations; destinations SHALL be the filesystem and S3-compatible stores.

#### Scenario: A full backup writes one file per group
- **WHEN** a full backup of a two-group cluster runs
- **THEN** two backup files and a manifest entry with type full and backup number 1 exist

#### Scenario: An incremental backup holds only newer versions
- **WHEN** an incremental backup follows a full one
- **THEN** it holds only versions newer than the full backup's read timestamp, with the same backup ID and number 2

#### Scenario: Filesystem and S3 backups are equivalent
- **WHEN** one cluster is backed up to the filesystem and to an S3-compatible store
- **THEN** both manifests list the same groups and predicates and both restore to the same data

### Requirement: Restores reproduce a point in the chain
A restore of a chain up to backup number `N` SHALL reproduce the cluster's state at that backup's read timestamp,
honour the drop operations recorded in the chain, assign each predicate to the group the manifest names, and raise
the UID and namespace leases above everything restored.

#### Scenario: A chain restores to its Nth backup
- **WHEN** a full backup and two incremental ones are restored up to number 2
- **THEN** the data equals the source's state at the second backup's read timestamp

#### Scenario: Drops in the chain are honoured
- **WHEN** a predicate dropped between two backups is restored from the chain
- **THEN** the restored cluster does not hold it

#### Scenario: New writes after a restore get fresh uids
- **WHEN** a restore completes and new nodes are created
- **THEN** none of their uids collides with a restored uid

### Requirement: Broken chains and mismatches are refused
A restore SHALL refuse a chain with a missing file, a gap in backup numbers or mixed backup IDs, an online restore
into a cluster whose groups differ from the manifest's, and an encrypted backup.

#### Scenario: A gap in the chain is refused
- **WHEN** backup number 2 of a chain is missing
- **THEN** a restore up to number 3 is refused

#### Scenario: An encrypted backup is refused
- **WHEN** a restore is given an encrypted Go Dgraph backup
- **THEN** it is refused with an error saying that encrypted backups are not supported
