# config-service Specification

## Purpose
The config service holding every role's settings: each target's files as a chain of immutable numbered revisions in an
ordered log, declared by one central file, validated whole by the same check in the service and the administrator's CLI
before any is stored or served, served over authenticated HTTP, and fetched by a role at start from five bootstrap
variables, so one validated source configures every role and a restart is how a role loads another revision.
## Requirements
### Requirement: One config service holds every role's settings
`ultragraphdb_config` SHALL hold the settings of every role of every deployment target, and every role SHALL read its
settings from it at start and from nowhere else. A target SHALL name its files by role and instance. A role's name, such as `storage`, SHALL
name the file its instances share; that name, `_` and a label, such as `storage_2`, SHALL name an instance and its file. A role
with no labelled instance SHALL have one instance, named as the role. An instance's settings SHALL be its role's file with
its own over them, key by key, rendered canonically, so two reads of one revision are byte-identical. The service SHALL be one process over one data directory, which it
SHALL hold locked, so a second process over the directory is refused. It SHALL be a statically linked musl binary in a
scratch image, running as the base's `nobody` (65534), and SHALL link no storage engine.

#### Scenario: An instance's file overrides its role's
- **WHEN** target `ci` holds `storage` with `partition_count = 2` and `storage_2` with `partition_first = 3`
- **THEN** `storage_2` receives both settings, and `storage_1`, whose file names `partition_first = 1`, receives its own

#### Scenario: A second process over the directory is refused
- **WHEN** a config service runs and another starts over the same data directory
- **THEN** the second exits with code 69 and an error naming the directory

#### Scenario: The image holds its binary alone
- **WHEN** the config service's image is built
- **THEN** it holds the statically linked binary on the scratch base, for amd64 and arm64, runs as `nobody`, and its
  dependency graph names neither `ultragraphdb_store_kv` nor fjall

### Requirement: A target's settings are immutable numbered revisions
Each target's settings SHALL be a chain of revisions numbered from 1, each holding the whole set of the target's files
and never changed once stored, and one revision of each target SHALL be its active one. Adding a revision SHALL name the
latest revision it was edited from, and SHALL be refused with status 409 naming the latest when another came first. Adding
a revision SHALL leave the active one as it was, and activating any stored revision, older or newer, SHALL be one durable
step. Every revision and activation SHALL be a record of an `OrderedLog`, the state rebuilt by replaying the log, and
acknowledged only once durable, so the replication stage 8 builds beneath that interface carries them unchanged. A revision's record SHALL hold its files as they were received, and a read of them SHALL return those bytes.

#### Scenario: A rollback activates an older revision
- **WHEN** target `ci` holds revisions 1 and 2, with 2 active, and an administrator activates 1
- **THEN** a role starting next receives revision 1, and revision 2 is still stored

#### Scenario: A revision edited from a stale one is refused
- **WHEN** two administrators edit from revision 3, and the first adds revision 4
- **THEN** the second's revision is refused with status 409 naming revision 4, and nothing is stored

#### Scenario: A restart holds what was acknowledged
- **WHEN** a revision is acknowledged and the service is killed and restarted
- **THEN** the revision is stored, and the active revision is the one last acknowledged

#### Scenario: A torn record is not a revision
- **WHEN** the service is killed while appending a revision it has not acknowledged
- **THEN** after a restart the revision is absent or whole, never partial

### Requirement: The config service validates every setting before it serves one
The config service SHALL check a set of files whole before it stores a revision, and SHALL check every target's active
revision at start, refusing to start with code 78 when one fails, so nothing it serves can be refused by a later check.
Every refusal SHALL name the target, the instance, the setting and the rule, and SHALL list every defect found, not the
first. It SHALL check:

| Check | Refused when |
|---|---|
| Parsing | a file is not the TOML subset, or names a table or key `ServiceConfig` does not hold |
| Names | an instance's name does not begin with a role's name, or an instance file has no instance of the target |
| Roles | the target has no frontend, more than one, or no storage instance |
| Required | a role's required setting is absent from an instance |
| Bounds | a value is outside its domain type's bound, by that type's own constructor, or `scan_stall_ms`, `read_retain_ms` or `held_bytes` is 0, or `scan_stall_ms` is above 30,000, the floor of the client's read timeout |
| Relations | `epoch_ms_min` is above `epoch_ms_max`, or `close_request_ms` is below twice `epoch_ms_max` |
| Identity | two instances name different `cluster.id` values |
| Frontend | an instance's `cluster.frontend_address` differs from the frontend's `frontend.role_address` in its scheme, its port or a Unix path; a host may differ, since a Service's name reaches a pod that binds its own |
| Partitions | the storage instances' partitions do not cover 1 to `partition_count` once each |
| Addresses | two listeners hold one address, a partition's derived address included: a named host or a Unix path across the target, a wildcard host within one instance, since each pod holds its own; a role address naming a wildcard host, since others dial it |
| Data | two instances whose addresses name one host, or Unix paths, hold one `data_dir` |
| Security | an endpoint names neither TLS nor `plaintext = true`, both, or half a certificate pair |
| Secrets | a path the settings name for a secret is not absolute, or `admin.key_sha256` is not 64 hexadecimal digits |
| Fixed | against the active revision: `cluster.id`, `cluster.partition_count`, or an instance's partitions or `data_dir` differ, while the placement is fixed |

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

### Requirement: One central file declares every settings file
The config service's `targets_file` setting SHALL name a required TOML file, the central file, holding one table per target,
each key a role or instance and each value a settings file relative to the central file's directory. At every start, before it
serves anything, the service SHALL read the central file and every file it declares and validate each target whole, and SHALL
refuse to start with code 78 when the central file is missing or unreadable, declares no target, declares a target with no file, or
declares a file that is not there, or fails a check of the requirement above. On a directory holding no revision of a target, the service
SHALL store the target's files as revision 1 and activate it. On a target holding a revision it SHALL leave the stored revisions as they
are, logging the target's name and active revision when the declared files differ from them, since from then on the API is the one
authority, and an edit made through it is saved by downloading it. The profiles `dev`, every role on one host over Unix sockets,
`dev-net`, the same over TCP, SHALL ship as central files with their settings, and the profile `kube`, each role a pod of the Helm
chart `deploy/helm/ultragraphdb` at its headless Service's name, SHALL be that chart's, checked as they are.

#### Scenario: No central file stops the start
- **WHEN** the service starts with `targets_file` naming a file that does not exist
- **THEN** it exits with code 78 and an error naming the setting and the path it looked at

#### Scenario: A central file declaring nothing stops the start
- **WHEN** the central file holds no target, or a target with no file
- **THEN** it exits with code 78, the error naming the central file and saying what it must declare, with the target when one is empty

#### Scenario: A declared file that is absent stops the start
- **WHEN** target `ci` declares `storage_1 = "ci_storage_1.toml"` and no such file exists
- **THEN** it exits with code 78, the error naming the target, the instance, the value as written and the path it resolved to

#### Scenario: Every absent file is named at once
- **WHEN** two declared files are absent and a third is malformed
- **THEN** the one error lists all three, so one run tells the administrator everything to fix

#### Scenario: A central file begins a target once
- **WHEN** a service starts declaring target `ci`, and again after a declared file changed validly
- **THEN** `ci` holds revision 1 alone, and the second start logs that the file differs and the revision still active

### Requirement: An administrator checks settings before starting anything
The validation SHALL live in the crate `ultragraphdb_common_config_check`, which reads a central file and its files and reports every
defect, with no store, no network and no key, and which the service and the CLI both call, so the two cannot disagree. The binary
`ultragraphdb_config_cli`, given a central file's path, SHALL print every defect, or each target with its instances when there is none,
and SHALL exit 0 when the settings pass and 78 when they do not. It SHALL also report, as a warning that does not fail the check, a
`.toml` file in the central file's directory that nothing declares, since that is usually a misspelled name. Where a check compares a
revision against the active one, only the service holds that revision, so those checks SHALL run there and the CLI SHALL say it did not
make them.

#### Scenario: The check finds what the service would refuse
- **WHEN** an administrator runs `ultragraphdb_config_cli` over settings the service would refuse
- **THEN** it prints the same defects the service would, and exits with code 78

#### Scenario: The check passes what the service would take
- **WHEN** an administrator runs it over valid settings
- **THEN** it lists each target with its instances, says it did not compare against stored revisions, and exits with code 0

#### Scenario: A stray file is pointed out
- **WHEN** the directory holds `ci_storge_1.toml`, which no target declares
- **THEN** the check warns naming the file, and its exit code is unchanged

### Requirement: An administrator restores a running service's settings into a folder
`ultragraphdb_config_cli restore <url> <admin token> <folder>` SHALL, with `--ca-file <file>` for a service whose certificate the system's roots do not verify, download the latest revision of every target, and no other revision, and write them under the folder as a central file, `targets.toml`,
written last, and each target's files in a folder of its name, so a config service can begin from the folder, each target's latest revision becoming its revision 1.
It SHALL say when a target's latest revision is not its active one, since a service beginning from the folder makes it active. A retired target's settings are run by no one, so it SHALL leave it out and say so. It SHALL write only into a folder
that is absent or empty, and SHALL check names from the service before writing any. It SHALL never print the token. It SHALL exit
0 once the folder passes the check, 69 when the service does not answer, 74 when the folder cannot take the files, and 78 otherwise.

#### Scenario: A restore rebuilds a folder a service can begin from
- **WHEN** an administrator adds a revision of a target through the API, activates none, and restores the service into a folder
- **THEN** the folder holds the latest revision's files alone, says which is active, and a fresh service over it serves those settings

#### Scenario: A restore leaves out a retired target
- **WHEN** a service holds a retired target and one with no active revision beside `ci`
- **THEN** the restore holds `ci` and the one with no active revision, and says it left out the retired one

#### Scenario: A restore with the read key writes nothing
- **WHEN** a restore is run with the read key
- **THEN** it exits with code 78 saying the token was refused, prints no token, and no folder appears

#### Scenario: A restore adds to no folder
- **WHEN** a restore names a folder holding a file
- **THEN** it exits with code 74 saying so, and the folder is as it was

#### Scenario: A restore of a service that is down says so
- **WHEN** a restore names a URL where nothing answers
- **THEN** it exits with code 69 naming the URL

### Requirement: A configuration error tells the administrator what to do
Every refusal of settings, from the service, the check or a role, SHALL state in one entry: where (the file and, when a parser knows it,
the line), which target, instance and setting, what was found, and what is required. Every entry SHALL be complete on its own, written for
an administrator who has not read the source, with no internal type name, no debug formatting and no stack. A value naming a secret SHALL
never be printed.

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
- **WHEN** `ci_frontend.toml` sets `storage.scan_stall_ms = 0`
- **THEN** the entry names `storage.scan_stall_ms`, the value 0 and the bound it must meet

#### Scenario: A retention of no time is refused
- **WHEN** `ci_storage_1.toml` sets `storage.read_retain_ms = 0`
- **THEN** the entry names `storage.read_retain_ms`, the value 0 and the bound it must meet

#### Scenario: A held-bytes bound of nothing is refused
- **WHEN** `ci_frontend.toml` sets `endpoint.held_bytes = 0`
- **THEN** the entry names `endpoint.held_bytes`, the value 0 and the bound it must meet

#### Scenario: A parked-bytes bound of nothing is refused
- **WHEN** `ci_storage_1.toml` sets `storage.held_bytes = 0`
- **THEN** the entry names `storage.held_bytes`, the value 0 and the bound it must meet

#### Scenario: A misspelled setting is named where it stands
- **WHEN** `ci_storage.toml` holds `partiton_count = 4` under `[storage]`
- **THEN** the entry names the file, the line, `storage.partiton_count`, and says no such setting exists

### Requirement: The config service serves its API over authenticated HTTP
The config service SHALL serve HTTP/1.1 at its `address`, over TLS through `ultragraphdb_transport_tls` unless its settings
name `plaintext = true`, and SHALL refuse a request without a bearer key with status 401 and one whose key's scope is too
narrow with status 403, comparing keys in constant time. It SHALL read two keys at start, each from its own file and at
least 32 bytes long and drawn at random by whoever provisions it: the read key, which reads settings, and the admin key, which also changes them. A key missing, unreadable
or too short SHALL stop the start with code 78. The API SHALL be:

| Method and path | Key | Does |
|---|---|---|
| `GET /v1/targets` | read | lists the targets, one line each: `<name> latest <n> active <n or none>`, followed by ` retired` for a retired one |
| `GET /v1/targets/{target}/instances/{instance}` | read | the instance's merged TOML at the active revision, its revision in `X-Revision` |
| `GET /v1/targets/{target}/revisions/{n}/instances/{instance}` | read | the same at revision `n` |
| `GET /v1/targets/{target}/revisions/{n}` | admin | revision `n`'s files |
| `GET /v1/targets/{target}/download` | admin | the active revision's files as a file to save, its revision in `X-Revision`, which the `POST` below takes back as they are |
| `POST /v1/targets/{target}/revisions?base={n}` | admin | adds a revision holding the files of the body; `base` 0 begins a target |
| `PUT /v1/targets/{target}/active` | admin | activates the revision whose decimal number the body holds, which also ends a retirement |
| `DELETE /v1/targets/{target}` | admin | retires the target: it serves no instance, and its revisions stay stored |

A revision's files SHALL travel as a sequence of a name, a space, the file's length in decimal, a line break, the file's bytes
and a line break. A revision replaces the whole set, so a file left out is removed. A request SHALL take at most 5 s and hold at
most 512 KiB, which a revision fits in one round of the log, and one past either bound SHALL be refused without keeping more of it. A client may still be sending when it is refused, and closing over unread bytes resets the connection, which destroys the refusal on its way. So after any refusal given before a request's body is whole, for its size, its head or its framing, or for its time, the service SHALL shut its sending side, then read and discard what still arrives until the client ends, or until 16 MiB or 5 s more have passed, and only then close. It SHALL serve at most 64 connections at once, as a role's admin listener does, a further one waiting to be accepted until one of them ends. The
service SHALL serve `/health` and `/ready` as the roles do, without a key. It SHALL serve no secret's value: settings name
secret files, and each role reads its own.

#### Scenario: A request without a key is refused
- **WHEN** a request reaches a target's instance with no bearer key
- **THEN** it is refused with status 401, and its body names no setting

#### Scenario: The read key changes nothing
- **WHEN** a request adds a revision with the read key
- **THEN** it is refused with status 403, and nothing is stored

#### Scenario: An unknown instance is told so
- **WHEN** a request names an instance the active revision does not hold
- **THEN** it is refused with status 404 naming the target, the revision and the instance

#### Scenario: A retired target serves nothing
- **WHEN** a target is retired and a role of it starts
- **THEN** the request is refused with status 404 saying the target is retired, and the target's revisions are still stored

#### Scenario: A key too short stops the start
- **WHEN** the service starts with an admin key of 8 bytes
- **THEN** it exits with code 78 naming the key's file and the 32 bytes it requires

#### Scenario: A target with no active revision says so
- **WHEN** a role asks for an instance of a target whose only revision has not been activated
- **THEN** it is refused with status 404 saying the target has no active revision

#### Scenario: An edit made through the API is downloaded and saved
- **WHEN** an administrator activates a revision through the API and downloads its target
- **THEN** the answer holds that revision's files as received, names the revision and a file to save them as, and taken back it adds a revision

#### Scenario: A download needs the admin key
- **WHEN** a request downloads a target with the read key
- **THEN** it is refused with status 403

#### Scenario: A target with no active revision has nothing to download
- **WHEN** an administrator downloads a target whose only revision has not been activated
- **THEN** it is refused with status 404 saying the target has no active revision

#### Scenario: An oversized request is refused
- **WHEN** a request's declared body is larger than the bound
- **THEN** it is refused with status 413 before its body is read, and nothing is stored

#### Scenario: A refusal reaches a client still sending
- **WHEN** a client goes on sending a body after the refusal of its declared size has arrived
- **THEN** each of its writes is taken, it reads status 413 and then the end of the connection, and nothing is stored

### Requirement: A role starts from five bootstrap variables
Every role SHALL take exactly these from its environment, and SHALL refuse any other variable beginning `ULTRAGRAPHDB_`,
naming it, with code 78:

| Variable | Holds | Required |
|---|---|---|
| `ULTRAGRAPHDB_CONFIG_URL` | the service's `https://host:port`, or `http://` for a plaintext service | yes |
| `ULTRAGRAPHDB_CONFIG_TARGET` | the deployment target | yes |
| `ULTRAGRAPHDB_CONFIG_INSTANCE` | this instance's name, which must begin with this binary's role | yes |
| `ULTRAGRAPHDB_CONFIG_KEY_FILE` | the file holding the read key | yes |
| `ULTRAGRAPHDB_CONFIG_CA_FILE` | the certificates that verify an `https` service | with `https` |

A role executing itself to load a revision SHALL set `ULTRAGRAPHDB_CONFIG_REVISION` on itself, which nothing else sets and a start takes in place of the active revision. A role or a config service started with `ULTRAGRAPHDB_PARENT_PID`, which a test harness and the supervisor each set to their own process, SHALL stop as on SIGTERM once that process is no longer its parent, so what a killed process started ends and frees its ports. A role SHALL ask the service for its instance's active revision, retrying an unreachable service until 60 s have passed,
then exit with code 69 and an error naming the URL, the target, the instance and the last failure. It SHALL NOT retry a
refusal: a key refused, or a target or instance unknown, SHALL exit with code 78 at once, since waiting mends neither. It
SHALL parse what it receives by `ServiceConfig`, whose tables and keys it holds, and check that the settings its role
requires are set, and leave every other check to the service. A role SHALL listen at nothing before it holds its
settings, since its addresses are among them, so its log and its exit code are the diagnostic until then.

#### Scenario: The service is down
- **WHEN** a storage process starts and no config service answers for 60 s
- **THEN** it exits with code 69 and an error naming the URL, the target and the instance

#### Scenario: The service starts late
- **WHEN** a storage process starts and the config service begins answering 5 s later
- **THEN** the process takes its settings and starts

#### Scenario: A wrong key is not retried
- **WHEN** a storage process starts with a key the service refuses
- **THEN** it exits with code 78 at once, naming the URL

#### Scenario: An instance of another role is refused
- **WHEN** the storage binary starts with `ULTRAGRAPHDB_CONFIG_INSTANCE=compute_1`
- **THEN** it exits with code 78 naming the instance and its role, without asking the service

#### Scenario: A stray variable is refused
- **WHEN** a role starts with `ULTRAGRAPHDB_STORAGE_PARTITION_COUNT=2` in its environment
- **THEN** it exits with code 78 naming the variable

#### Scenario: A role ends with the process it names
- **WHEN** a role starts with `ULTRAGRAPHDB_PARENT_PID` naming a process that is not its parent
- **THEN** it stops as on SIGTERM and exits with code 0

#### Scenario: The config service ends with the process it names
- **WHEN** a config service starts with `ULTRAGRAPHDB_PARENT_PID` naming a process that is not its parent
- **THEN** it stops as on SIGTERM and exits with code 0

#### Scenario: The config service takes a port held a moment
- **WHEN** a config service starts at an address another process holds, which lets it go within the bound
- **THEN** it logs each retry naming the address, binds it, and answers `/health`

#### Scenario: A new instance takes what the others hold
- **WHEN** a new storage process of a newer release starts during a rolling update, the settings unchanged
- **THEN** it receives the active revision the running instances report

### Requirement: A role reports its revision and loads another by restarting
A role's `/health` SHALL answer with its target, its instance and the revision it runs. A role SHALL hold settings fixed for
its process's life, and SHALL serve `POST /admin/load-config`, whose body holds a revision's decimal number, on a listener of its own at `[admin]
address`, under the same TLS rule as the service's, authorising the admin key against the SHA-256 digest its `[admin] key_sha256`
holds, so no node holds a key that can change settings. A role taking the request
SHALL fetch that revision of its instance and check it as at a start, and on any failure SHALL refuse the request and
run on, unchanged. Otherwise it SHALL answer 202, stop through its ordinary shutdown, and execute itself again in the same
process, so whatever supervises it sees no exit, starting at that revision. No component SHALL reconfigure more than the
node asked: the service SHALL call no role, and an operator SHALL choose the order in which nodes load a revision.

#### Scenario: A health answer names the revision
- **WHEN** a role runs revision 7 of target `ci` as `storage_1`
- **THEN** its `/health` body names `ci`, `storage_1` and 7

#### Scenario: A load restarts the node at the revision
- **WHEN** an administrator asks `storage_1` to load revision 8, which the service holds
- **THEN** it answers 202, flushes as at SIGTERM, keeps its process id, and its `/health` names revision 8

#### Scenario: A load without the admin key changes nothing
- **WHEN** a request to load a revision carries the read key
- **THEN** it is refused with status 403, and the role runs on

#### Scenario: A load of a revision that cannot be fetched changes nothing
- **WHEN** a role is asked to load a revision the service does not hold, or while the service is down
- **THEN** the request is refused naming the revision and why, and the role runs on at its revision

#### Scenario: A loaded revision does not outlive a crash unless active
- **WHEN** a role loaded revision 8 while 7 is active, and then crashes and restarts
- **THEN** it starts at revision 7, and its `/health` says so

### Requirement: A rolling update keeps settings and releases compatible
A release SHALL start on settings that do not name a setting it adds, each new setting taking a stated default or
absent meaning, and SHALL keep accepting a setting until a release after all instances stopped naming it, so the same revision
serves two neighbouring releases. A rolling update SHALL upgrade the config service first, then the roles, and a revision SHALL
name a new setting only once every instance runs a release holding it. A role SHALL still refuse a setting it does not
hold, so a revision breaking this order stops that role with code 78 at its next start, naming the setting.

#### Scenario: Two releases share a revision
- **WHEN** release N+1 adds a setting, and N+1 instances start beside N instances on the active revision, which does not name it
- **THEN** both run, and report the same revision

#### Scenario: A setting named too early is named back
- **WHEN** an active revision names a setting release N does not hold, and an instance of release N restarts
- **THEN** it exits with code 78 naming the setting

