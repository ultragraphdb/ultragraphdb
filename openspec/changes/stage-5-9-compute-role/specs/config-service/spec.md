## MODIFIED Requirements

### Requirement: The config service validates every setting before it serves one
The config service SHALL check a set of files whole before it stores a revision, and SHALL check every target's active
revision at start, refusing to start with code 78 when one fails, so nothing it serves can be refused by a later check.
Every refusal SHALL name the target, the instance, the setting and the rule, and SHALL list every defect found, not the
first. It SHALL check:

| Check | Refused when |
|---|---|
| Parsing | a file is not the TOML subset, or names a table or key `ServiceConfig` does not hold |
| Names | an instance's name does not begin with a role's name, or an instance file has no instance of the target |
| Roles | the target has no compute instance or no storage instance |
| Compute | a compute instance lacks `compute.advertised_address`, or the address is not `tcp://host:port` as `PROTOCOL.md` section 5.4 states it; its `compute.tags` hold none, more than eight, a repeat, or a tag that is not 1 to 32 bytes of `a` to `z`, `0` to `9` and `_`; two compute instances advertise one address; the target's compute instances hold more than 256 distinct tags |
| System | no storage instance holds a `[system]` table, or more than one does; an instance holding it lacks `system.role_address`, `system.data_dir`, a `[metadata]` or a `[sequencer]` table, or holds no partition, which stage 8 may lift; an instance not holding it, a compute instance included, holds a `[metadata]` or `[sequencer]` table |
| Required | a role's required setting is absent from an instance |
| Bounds | a value is outside its domain type's bound, by that type's own constructor, or `scan_stall_ms` or `held_bytes` is 0, or `scan_stall_ms` is above 30,000, the floor of the client's read timeout, or a `[tiles]` setting is outside its bound (the requirement below) |
| Relations | `epoch_ms_min` is above `epoch_ms_max`, or `close_request_ms` is below twice `epoch_ms_max` |
| Identity | two instances name different `cluster.id` values |
| Tiles | two storage instances hold different values of one `[tiles]` setting, an absent one counted as its default |
| System address | an instance's `cluster.system_address` differs from the hosting instance's `system.role_address` in its scheme, its port or a Unix path; a host may differ, since a Service's name reaches a pod that binds its own |
| Partitions | the storage instances' partitions do not cover 1 to `partition_count` once each |
| Addresses | two listeners hold one address, a partition's derived address included: a named host or a Unix path across the target, a wildcard host within one instance, since each pod holds its own; a role address naming a wildcard host, since others dial it |
| Data | two instances whose addresses name one host, or Unix paths, hold one `data_dir`; a `system.data_dir` is equal to, inside or containing a `data_dir` of an instance on its host |
| Security | an endpoint names neither TLS nor `plaintext = true`, both, or half a certificate pair |
| Secrets | a path the settings name for a secret is not absolute, or `admin.key_sha256` is not 64 hexadecimal digits |
| Fixed | against the active revision: `cluster.id`, `cluster.partition_count`, an instance's partitions or `data_dir`, or the instance holding `[system]` and its `system.data_dir` differ, while the placement is fixed; or a `[tiles]` setting differs, before stage 9.5 |

A secret file's contents and a path's existence SHALL be left to the role reading them, on its own host. A role's start can
therefore still fail on what only its host knows, such as a port another process holds, and its log and exit code say so.

#### Scenario: A broken file refuses the revision
- **WHEN** an administrator adds a revision whose `storage_1` lacks `data_dir` and whose `compute_1` names `partiton`
- **THEN** it is refused with status 422 listing both defects, each naming its instance and setting, and nothing is stored

#### Scenario: A partition held twice is refused
- **WHEN** `storage_1` and `storage_2` both hold partition 2
- **THEN** the revision is refused naming partition 2 and both instances

#### Scenario: A partition no one holds is refused
- **WHEN** `partition_count` is 4 and the storage instances hold partitions 1 to 3
- **THEN** the revision is refused naming partition 4

#### Scenario: Two hosting instances are refused
- **WHEN** `storage_1` and `storage_2` both hold a `[system]` table
- **THEN** the revision is refused naming `system` and both instances

#### Scenario: A target with no hosting instance is refused
- **WHEN** no storage instance holds a `[system]` table
- **THEN** the revision is refused naming `system` and the target

#### Scenario: The retired role's settings are refused by name
- **WHEN** any file holds a `[frontend]` table, `compute.role_address` or `cluster.frontend_address`
- **THEN** the revision is refused naming the table or the setting as unknown

#### Scenario: A compute instance without tags is refused
- **WHEN** `compute_1` holds `compute.tags = []`
- **THEN** the revision is refused naming `compute.tags` and the instance

#### Scenario: Two compute instances advertising one address are refused
- **WHEN** `compute_1` and `compute_2` both hold `compute.advertised_address = "tcp://db.example:7080"`
- **THEN** the revision is refused naming the address and both instances

#### Scenario: A target of several compute instances is accepted
- **WHEN** a target holds `compute_1`, `compute_2` and `storage_1`, each compute instance with its own advertised address
- **THEN** the revision is stored

#### Scenario: The books' directory inside a partition's is refused
- **WHEN** `storage_1` holds `storage.data_dir = "/data/storage"` and `system.data_dir = "/data/storage/system"`
- **THEN** the revision is refused naming both settings and the instance

#### Scenario: A system address that differs is refused
- **WHEN** `compute_1` names a `cluster.system_address` whose port differs from `storage_1`'s `system.role_address`
- **THEN** the revision is refused naming both addresses and both instances

#### Scenario: Two listeners on one address are refused
- **WHEN** `storage_1`'s second partition derives the address `compute_1` listens at
- **THEN** the revision is refused naming the address and both instances

#### Scenario: A changed cluster identity is refused
- **WHEN** a revision names a `cluster.id` other than the active revision's
- **THEN** it is refused naming both values

#### Scenario: A broken declared file stops the start
- **WHEN** the config service starts on a fresh directory with a declared file failing a check
- **THEN** it exits with code 78, lists every defect, stores nothing and serves nothing

#### Scenario: An active revision a release no longer accepts stops the start
- **WHEN** a release starts over a directory whose active revision names a setting the release does not hold
- **THEN** it exits with code 78 naming the target, the revision and the setting, and the release before it, started
  over the same directory, serves as it did

### Requirement: A configuration error tells the administrator what to do
Every refusal of settings, from the service, the check or a role, SHALL state in one entry: where (the file and, when a
parser knows it, the line), which target, instance and setting, what was found, and what is required. Every entry SHALL
be complete on its own, written for an administrator who has not read the source, with no internal type name, no debug
formatting and no stack. A value naming a secret SHALL never be printed.

#### Scenario: A value out of bounds says what it requires
- **WHEN** `ci_storage_1.toml` sets `storage.partition_count = 0`
- **THEN** the entry names `ci_storage_1.toml`, its line, target `ci`, instance `storage_1`, the setting, the value 0 and the bound it must meet

#### Scenario: A Scan's stall of no time is refused
- **WHEN** `ci_storage_1.toml` sets `storage.scan_stall_ms = 0`
- **THEN** the entry names `storage.scan_stall_ms`, the value 0 and the bound it must meet

#### Scenario: A stall past the client's read timeout is refused
- **WHEN** `ci_storage_1.toml` sets `storage.scan_stall_ms = 30001`
- **THEN** the entry names `storage.scan_stall_ms`, the value 30001 and the bound it must meet

#### Scenario: A hung compute's stall of no time is refused
- **WHEN** `ci_compute.toml` sets `storage.scan_stall_ms = 0`
- **THEN** the entry names `storage.scan_stall_ms`, the value 0 and the bound it must meet

#### Scenario: A held-bytes bound of nothing is refused
- **WHEN** `ci_compute.toml` sets `endpoint.held_bytes = 0`
- **THEN** the entry names `endpoint.held_bytes`, the value 0 and the bound it must meet

#### Scenario: A parked-bytes bound of nothing is refused
- **WHEN** `ci_storage_1.toml` sets `storage.held_bytes = 0`
- **THEN** the entry names `storage.held_bytes`, the value 0 and the bound it must meet

#### Scenario: A misspelled setting is named where it stands
- **WHEN** `ci_storage.toml` holds `partiton_count = 4` under `[storage]`
- **THEN** the entry names the file, the line, `storage.partiton_count`, and says no such setting exists
