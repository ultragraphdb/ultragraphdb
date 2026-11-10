## ADDED Requirements

### Requirement: Every storage instance runs under the cluster's one `[tiles]` table
A storage role SHALL take the `[tiles]` table of its revision, which the config service holds equal across the
cluster's storage instances and fixed before stage 9.5 (config-service, "The `[tiles]` table is the same on every
storage instance"), and SHALL open each partition's grid under the derivation version those settings give (tile-grid,
"The derivation version"), so every replica of every partition writes and logs under one derivation version. A grid
whose superblock names another derivation version SHALL be refused with `DerivationMismatch`, naming both versions,
and its partition SHALL not be served: the storage instance SHALL exit with code 78 (configuration error) and SHALL
never answer `/ready` ("Exit codes name the cause"; `STORAGE.md` section 17.2; T48).

#### Scenario: Two storage processes derive one version
- **WHEN** one storage process hosting partitions 1 and 2 and another hosting 3 and 4 start from one revision
- **THEN** every partition's grid superblock and every round it logs name the same derivation version

#### Scenario: A grid written under other settings is not served
- **WHEN** a storage process starts over a partition's grid whose superblock names a derivation version other than its
  `[tiles]` settings give
- **THEN** the grid's open is refused with `DerivationMismatch` naming both versions, nothing is replayed into it, the
  process exits with code 78, and its `/ready` never answers

## MODIFIED Requirements

### Requirement: Each role is its own binary and image
The roles SHALL be three binaries under `ultragraphdb/`: `ultragraphdb_frontend` (the client endpoint,
authentication, the metadata group and the sequencer), `ultragraphdb_compute` (quad-pattern reads) and
`ultragraphdb_storage` (partitions). Each SHALL link only its role's crates and SHALL ship as its own multi-arch OCI
image for amd64 and arm64 on the Wolfi glibc base, running as user 65532 with no shell. A fourth binary,
`ultragraphdb_supervisor`, SHALL be the supervisor of the development image, which holds the three role binaries, the
config service (config-service) and the supervisor on the same base.

#### Scenario: A role image holds its binary alone
- **WHEN** the storage image is built
- **THEN** it holds the storage binary as its entrypoint on the glibc base, for amd64 and arm64, runs as user 65532, and
  holds no shell and no other role's binary

#### Scenario: The frontend binary carries no storage engine
- **WHEN** the frontend binary's dependency graph is listed
- **THEN** it names neither `ultragraphdb_store_tile` nor `ultragraphdb_store_grid`

### Requirement: Each role runs one runtime per core
Each role SHALL run one compio runtime per core it uses, a storage partition on its own, sharing no mutable state with
another; what crosses between them SHALL go by message. A storage partition's engine SHALL run on its runtime's thread,
apply, compaction, checkpoints and reads alike in the partition's turns (partitions, "A partition's core runs in
turns"), and the storage role SHALL start no thread for its store beside its partitions' runtimes: a grid's extent
allocation and hole punching SHALL run on compio's blocking pool (`specification/storage/STORAGE.md` section 2; T10).
`[runtime] pin_cores` SHALL ask each runtime to bind to one CPU, chosen from those the process is permitted, as a
container's cpuset restricts them. Pinning SHALL be a performance choice and never a condition of correctness: where
the platform cannot bind a thread, as macOS cannot, offering only a cache-locality hint, the role SHALL run unpinned and
say so. The frontend SHALL accept and serve clients on each of its runtimes, one per CPU it may use, on one client
address bound once: its role address, its connections back and its readiness on the first, each role that joins made
known to every one, each dialling its own connections to the other roles, and its counts of failed authentications
held by its owner for all of them (client-endpoint, "Connections authenticate against the metadata group's
principals").
At start each role SHALL log, per runtime, the CPU it asked for and what it has, read back from the operating system where
it can be.

#### Scenario: A pinned role reports what it holds
- **WHEN** a role starts with `pin_cores = true` on a platform that allows binding
- **THEN** it logs each runtime's CPU, read back from the operating system, each within the permitted set

#### Scenario: A platform refusing to pin still serves
- **WHEN** a role starts with `pin_cores = true` where binding is unavailable
- **THEN** it logs that it runs unpinned, and serves requests

#### Scenario: The frontend serves clients on every core
- **WHEN** 32 clients each commit a write over a connection of their own to a frontend of two partitions
- **THEN** every write is committed, the frontend logs a runtime per CPU, and more than one of its cores relays the writes

#### Scenario: Storage's store runs on its partitions' threads
- **WHEN** a storage process hosting two partitions commits writes until each partition has compacted and checkpointed
- **THEN** every apply, rewrite and checkpoint step ran on its partition's runtime thread, and the process started no
  other thread for its store

### Requirement: Readiness, health and shutdown
Each role SHALL serve `/health`, answering once it holds its settings (config-service) and has bound every listener it serves from its
start, so that `/health` answering says a connection to such an address is accepted: the frontend its role and client addresses. Until
then `/health` SHALL answer 503. Storage binds its partitions' addresses once they have recovered, with `/ready`, so its `/health`
answers while it recovers and a peer dialling it meanwhile is refused at once. Each role SHALL also serve `/ready`, answering once its dependencies are
reachable: the frontend once its metadata group and sequencer have recovered and a storage partition answers, storage
once its partitions have recovered their logs, compute once a frontend and a storage process answer. On SIGTERM the
frontend SHALL stop accepting connections and finish its open requests, compute SHALL drain, and storage SHALL sync what
its logs hold and apply it before it exits, taking the checkpoints whose positions those rounds reach, as at any time,
and none for the shutdown itself (tile-grid, "Checkpoint positions"), so its next start replays the rounds above its
last checkpoint.

#### Scenario: Storage is not ready before recovery
- **WHEN** a storage process restarts with 10,000 rounds to replay
- **THEN** `/health` answers once it holds its settings, and `/ready` answers only after the replay

#### Scenario: Storage flushes on shutdown
- **WHEN** storage receives SIGTERM after acknowledging commits
- **THEN** every acknowledged commit is in its synced log before it exits, and after a restart, which replays the rounds
  above its last checkpoint, every acknowledged commit reads back
