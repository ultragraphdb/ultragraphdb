## ADDED Requirements

### Requirement: Backups write the port's format
A backup SHALL read every group at one read timestamp leased from Zero and write, under
`<root>/<backup_id>/<backup_num>/`, each group's records in strictly ascending key order as parts of at most 256 MiB
(`g<group>-<part>.ugb`), and then `manifest.json`. A part SHALL hold a header (magic `UGBK`, format version, group,
backup identifier and number), LZ4-compressed frames each carrying its lengths and the CRC32C of its compressed
bytes, and a trailer (magic `UGBE`, record count, last key). A record SHALL be `put` with the key's posting list rolled up
at the read timestamp, or `delete`. A full backup SHALL hold every key; an incremental backup SHALL hold every key whose
newest version at or below its read timestamp is above the previous backup's, as `delete` when that version is a
tombstone. The manifest SHALL hold `writer: "ultragraphdb"`, the format version, `backup_id`, `backup_num`, `type`,
`since_ts`, `read_ts`, the drop operations since the previous backup, the uid and namespace leases' maxima, and per group
its predicates and its parts with size, record count, first and last key and SHA-256. Destinations SHALL be the
filesystem and S3-compatible stores, and no object SHALL be written twice.

#### Scenario: A full backup writes sorted parts per group
- **WHEN** a full backup of a two-group cluster runs
- **THEN** each group's parts hold every key of the group in ascending order, and the manifest has type full, backup
  number 1 and every part's hash

#### Scenario: An incremental backup holds changed and deleted keys
- **WHEN** an incremental backup follows a full one after one key was changed and another deleted
- **THEN** it holds a `put` for the changed key and a `delete` for the deleted one and no other key, with the same backup
  ID and number 2

#### Scenario: Filesystem and S3 backups are equivalent
- **WHEN** one cluster is backed up to the filesystem and to an S3-compatible store
- **THEN** the two backups hold the same parts with the same hashes and both restore to the same data

### Requirement: A backup exists once its manifest is written
A backup SHALL write its manifest after every part, atomically (a rename on a filesystem, one PUT on S3). Listing,
restore and incremental backups SHALL ignore a backup without a manifest, and a later backup SHALL remove the parts an
unpublished backup left under its number.

#### Scenario: A backup interrupted before its manifest leaves the chain unchanged
- **WHEN** a backup fails after writing some parts and before its manifest
- **THEN** listing shows the chain as before, a restore ignores the parts, and the next incremental backup takes that
  number

### Requirement: Restores reproduce a point in the chain
A restore of a chain up to backup number `N` SHALL reproduce the cluster's state at that backup's read timestamp: per
group, it merges the chain's sorted streams so that the newest backup's record for a key wins and a `delete` removes the
key, skips a record of backup `j` when a drop recorded in a backup `m` with `j < m ≤ N` covers it, ingests the merged
stream into a fresh store in key order, assigns each predicate to the group the manifest names, and, online and offline
alike, raises the uid and namespace leases to the manifest's maxima.

#### Scenario: A chain restores to its Nth backup
- **WHEN** a full backup and two incremental ones are restored up to number 2
- **THEN** the data equals the source's state at the second backup's read timestamp

#### Scenario: Drops in the chain are honoured
- **WHEN** a predicate dropped between two backups is restored from the chain
- **THEN** the restored cluster does not hold it

#### Scenario: A key deleted between backups stays deleted
- **WHEN** a key present in the full backup is deleted before the first incremental one and the chain is restored
- **THEN** the restored cluster does not hold the key

#### Scenario: New writes after a restore get fresh uids
- **WHEN** an online or an offline restore completes and new nodes are created
- **THEN** none of their uids collides with a restored uid

### Requirement: A failed restore leaves the target unchanged
A restore SHALL write into staged stores and switch to them only after every group has ingested and verified its
stream: an offline restore renames a new store directory into place, and an online restore switches every group once
all have succeeded. It SHALL check free space against the manifest's sizes before it starts.

#### Scenario: A corrupt part fails the restore and keeps the data
- **WHEN** an online restore meets a part whose frame CRC or SHA-256 does not match
- **THEN** the restore fails naming the part, and the cluster holds and serves its data as before

### Requirement: Broken chains, damaged parts and foreign backups are refused
A restore SHALL refuse:
- a chain with mixed backup identifiers, a gap in backup numbers, or fewer backups than the number requested;
- a part that is missing or whose size, hash, record count or key order differs from its manifest;
- an online restore into a cluster whose groups differ from the manifest's;
- a manifest without `writer: "ultragraphdb"` or with a format version it does not know.

#### Scenario: A gap in the chain is refused
- **WHEN** backup number 2 of a chain is missing
- **THEN** a restore up to number 3 is refused

#### Scenario: Keys out of order are refused
- **WHEN** a part holds a key that does not sort after the key before it
- **THEN** the restore is refused naming the part and the offset

#### Scenario: A backup written by Go Dgraph is refused
- **WHEN** a restore is given a directory whose manifest lacks `writer: "ultragraphdb"`
- **THEN** it is refused with an error saying that only backups written by ultragraphdb are restored

### Requirement: A backup chain can be verified without restoring it
`ultragraph backup verify` SHALL read a chain up to a backup number as restore does and check every frame, hash,
record count, key order and string value, writing nothing, and SHALL report the first defect with its part and offset.

#### Scenario: Verifying a sound chain succeeds
- **WHEN** a chain of a full and two incremental backups is verified
- **THEN** verification succeeds and reports each backup's record count

#### Scenario: Verifying a truncated part fails
- **WHEN** a chain whose last part was truncated is verified
- **THEN** verification fails naming that part

### Requirement: A backup holds only valid UTF-8 strings
A backup SHALL check every string value it writes for valid UTF-8 before writing it, and SHALL fail, naming the key,
when one is not. The store refuses such values at mutation time, so the check guards the backup against a defect
upstream; a restore then reads only valid strings, and text analysis on restore takes them as `&str` (ledger 17, 18,
31).

#### Scenario: A backup refuses a string value that is not UTF-8
- **WHEN** a backup meets a string value whose bytes are not valid UTF-8
- **THEN** the backup fails with an error naming the value's key and publishes no manifest

#### Scenario: A backup of valid strings succeeds
- **WHEN** a predicate holding `İstanbul` and `日本` is backed up and restored
- **THEN** the restored values equal the originals
