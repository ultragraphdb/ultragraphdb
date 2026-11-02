## ADDED Requirements

### Requirement: Each role is its own binary and image
The roles SHALL be three binaries under `ultragraphdb_bin`: `ultragraphdb_frontend` (the client endpoint,
authentication, the metadata group and the sequencer), `ultragraphdb_compute` (quad-pattern reads) and
`ultragraphdb_storage` (partitions). Each SHALL link only its role's crates and SHALL ship as its own multi-arch OCI
image for amd64 and arm64 on the Wolfi glibc base, running as user 65532 with no shell. A fourth binary,
`ultragraphdb_dev`, SHALL be the supervisor of the development image, which holds the three role binaries and the
supervisor on the same base.

#### Scenario: A role image holds its binary alone
- **WHEN** the storage image is built
- **THEN** it holds the storage binary as its entrypoint on the glibc base, for amd64 and arm64, runs as user 65532, and
  holds no shell and no other role's binary

#### Scenario: The frontend binary carries no storage engine
- **WHEN** the frontend binary's dependency graph is listed
- **THEN** it names neither `ultragraphdb_store_kv` nor fjall

### Requirement: Roles meet through the transport at their addresses
Roles SHALL exchange the binary protocol's frames through the transport, at addresses from configuration: `tcp://`
across hosts, `unix://` within a container. Every frame SHALL be encoded, bounds-checked and version-negotiated on both
carriers, and no role SHALL ask which carrier it runs on.

#### Scenario: The development container uses Unix sockets
- **WHEN** the development image starts with its default profile
- **THEN** the frontend, compute and storage processes connect through `unix:///run/ultragraph/<role>.sock`, and a
  write and a read through the frontend succeed

#### Scenario: The same frames cross TCP
- **WHEN** the three roles run in three containers configured with `tcp://` addresses
- **THEN** the same write and read succeed, and the frames each role exchanges are byte-identical to the Unix-socket
  run's, apart from the transport's own framing

### Requirement: The supervisor starts, stops and reports the roles
The supervisor SHALL start the frontend, then storage, then compute, each once its predecessor reports ready; SHALL
restart no role on its own; SHALL forward SIGTERM and SIGINT; SHALL stop the roles in the shutdown order; and SHALL exit
with the exit code of the first role that failed, or 0 after a clean shutdown.

#### Scenario: A failing role ends the container with its code
- **WHEN** the storage process exits with code 20 while the others run
- **THEN** the supervisor stops the frontend and compute and exits with code 20

#### Scenario: SIGTERM shuts down in order
- **WHEN** the supervisor receives SIGTERM
- **THEN** the frontend stops accepting work first, compute drains next, storage flushes last, and the supervisor exits 0

### Requirement: Readiness, health and shutdown
Each role SHALL serve `/health`, answering once its process runs, and `/ready`, answering once its dependencies are
reachable: the frontend once its metadata group and sequencer have recovered and a storage partition answers, storage
once its partitions have recovered their logs, compute once a frontend and a storage process answer. On SIGTERM the
frontend SHALL stop accepting connections and finish its open requests, compute SHALL drain, and storage SHALL apply
and sync what its logs hold before it exits.

#### Scenario: Storage is not ready before recovery
- **WHEN** a storage process restarts with 10,000 rounds to replay
- **THEN** `/health` answers at once and `/ready` answers only after the replay

#### Scenario: Storage flushes on shutdown
- **WHEN** storage receives SIGTERM after acknowledging commits
- **THEN** every acknowledged commit is applied to its store before it exits, and a restart replays nothing

### Requirement: Configuration is explicit and validated at start
Each role SHALL read its settings from one standard TOML file, parsed by `ultragraphdb_tool_toml` (settings-toml),
with an environment variable
`ULTRAGRAPHDB_<TABLE>_<KEY>` overriding the file's value, SHALL refuse to start on an unknown setting, a malformed
file, a missing required setting or a value out of its bounds, naming the setting and the bound, and SHALL exit with
the configuration code. The profiles `dev`, `dev-net`, `frontend`, `compute` and `storage` SHALL be presets of these
settings, shipped as TOML files. Each role's data SHALL live under one directory per role and replica, laid out as on a cluster node.

#### Scenario: An unknown setting stops the start
- **WHEN** the storage binary starts with a TOML file whose `[storage]` table holds the key `partiton_count`
- **THEN** it exits with the configuration code and an error naming `partiton_count`

#### Scenario: The data layout matches a node's
- **WHEN** the development container has started
- **THEN** `/data` holds `meta/`, `seq/` and `storage/p1/r1/`, the paths a cluster node uses for the same roles

### Requirement: io_uring where allowed, polling where not
Each role SHALL use io_uring when the kernel allows it and SHALL fall back to compio's polling driver when io_uring
system calls are refused, logging which it uses at start. The role images SHALL ship a seccomp profile that allows
`io_uring_setup`, `io_uring_enter` and `io_uring_register` beside the container runtime's defaults.

#### Scenario: A refused io_uring falls back
- **WHEN** a role starts under Docker's default seccomp profile
- **THEN** it starts on the polling driver, logs that it does, and serves requests

#### Scenario: The seccomp profile enables io_uring
- **WHEN** a role starts under the shipped seccomp profile
- **THEN** it starts on io_uring and logs that it does

### Requirement: Exit codes name the cause
Each role and the supervisor SHALL exit with a code naming the cause: 0 after a clean shutdown, a code for a
configuration error, one for a failed store or log (the component stopped on an operating error), one for an invariant
violation, and one for a lost dependency that does not return within its deadline. The codes SHALL be documented and
stable.

#### Scenario: An invariant violation exits with its code
- **WHEN** a storage partition detects an invariant violation in a release build
- **THEN** the partition stops, the process shuts down through its ordinary path, and it exits with the invariant code
