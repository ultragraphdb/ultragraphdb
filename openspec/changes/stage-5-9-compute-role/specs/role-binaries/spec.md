## MODIFIED Requirements

### Requirement: Each role is its own binary and image
The roles SHALL be two binaries under `ultragraphdb/`: `ultragraphdb_compute` (the client endpoint, the relay of
authentication, and the execution of reads) and `ultragraphdb_storage` (partitions and, in the instance its settings
name, the system group with the metadata state machine and the sequencer; system-group, "One storage instance hosts
the system group"). Each SHALL link only its role's crates and SHALL ship as its own multi-arch OCI image for amd64
and arm64 on the Wolfi glibc base, running as user 65532 with no shell. A third binary, `ultragraphdb_supervisor`,
SHALL be the supervisor of the development image, which holds the two role binaries, the config service
(config-service) and the supervisor on the same base. No binary, image, crate or settings table SHALL carry the name
`frontend`.

#### Scenario: A role image holds its binary alone
- **WHEN** the storage image is built
- **THEN** it holds the storage binary as its entrypoint on the glibc base, for amd64 and arm64, runs as user 65532, and
  holds no shell and no other role's binary

#### Scenario: The compute binary carries no storage engine and no book
- **WHEN** the compute binary's dependency graph is listed
- **THEN** it names neither `ultragraphdb_store_kv`, fjall, `ultragraphdb_common_system`, `ultragraphdb_cluster_metadata`,
  `ultragraphdb_cluster_sequencer` nor `ultragraphdb_transactions_log`

#### Scenario: Nothing is named frontend
- **WHEN** the workspace's members, the images, the chart and the profiles are searched for `frontend`
- **THEN** none is found

### Requirement: Roles meet through the transport at their addresses
Roles SHALL exchange the binary protocol's frames through the transport, at addresses from configuration: `tcp://`
across hosts, `unix://` within a container. Every frame SHALL be encoded, bounds-checked and version-negotiated on both
carriers, and no role SHALL ask which carrier it runs on.

#### Scenario: The development container uses Unix sockets
- **WHEN** the development image starts with its default profile
- **THEN** the system group listens at `unix:///run/ultragraph/system.sock`, each partition at `unix:///run/ultragraph/storage-p<partition>.sock`, compute listens at no role address, and a write and a read through compute succeed

#### Scenario: The same frames cross TCP
- **WHEN** the two roles run in two containers configured with `tcp://` addresses
- **THEN** the same write and read succeed, and the read's rows are byte-identical to the Unix-socket run's

### Requirement: The supervisor starts, stops and reports the roles
The supervisor SHALL start the config service and wait for its `/ready`, then start storage and wait for its `/ready`,
since storage hosts the system group and depends on no other role, and then start compute and wait for its `/ready`.
Compute SHALL retry dialling the system group until its deadline, so it does not need the order to hold. The supervisor
SHALL restart no role on its own; SHALL draw the config service's two keys at random at the development image's first
start, keeping them in its data directory, and the bootstrap administrator's password before the cluster's first start
alone, since the system group's host reads it once and the operator then removes it; SHALL refuse with code 78 a target
whose instances name more than one `role_secret_file`, since the roles of one container share one role secret; SHALL
forward SIGTERM and SIGINT; SHALL stop the roles in the shutdown order; SHALL set `ULTRAGRAPHDB_PARENT_PID` to its own
process for everything it starts, the config service included, so killed outright it orphans nothing, and a supervisor
at PID 1 has no parent to lose, its death ending the container; SHALL itself stop as on SIGTERM once the process its own
`ULTRAGRAPHDB_PARENT_PID` names is no longer its parent; and SHALL exit with the exit code of the first role that
failed, 128 plus the signal's number for a role a signal killed, or 0 after a clean shutdown.

#### Scenario: The start order reaches readiness
- **WHEN** the supervisor starts the config service and the two roles
- **THEN** the config service answers first, storage's `/ready` answers before compute is started, compute's answers last, and no role waited on one that was waiting on it

#### Scenario: A failing role ends the container with its code
- **WHEN** the storage process exits with code 74 while the others run
- **THEN** the supervisor stops compute and exits with code 74

#### Scenario: A role killed by a signal is reported as a shell would
- **WHEN** the compute node is killed by SIGKILL while the others run
- **THEN** the supervisor stops storage and exits with code 137

#### Scenario: SIGTERM shuts down in order
- **WHEN** the supervisor receives SIGTERM
- **THEN** compute stops accepting work and drains first, storage flushes last, and the supervisor exits 0

#### Scenario: Two role secret files in one container are refused
- **WHEN** the supervisor starts on a target whose storage file names another `role_secret_file` than its compute file's
- **THEN** it exits with code 78, the error naming both files, and started no role

#### Scenario: A bootstrap password removed after the first start stays removed
- **WHEN** the development container has started, the bootstrap administrator's password file is removed, and the container
  starts again over the same data directory
- **THEN** no file stands at the password's path, and the restart reached readiness

#### Scenario: A killed supervisor orphans no role
- **WHEN** the supervisor is killed by SIGKILL while every role runs
- **THEN** the config service and the two roles each stop as on SIGTERM and free their ports

#### Scenario: The supervisor ends with the process it names
- **WHEN** the supervisor starts with `ULTRAGRAPHDB_PARENT_PID` naming a process that is not its parent
- **THEN** it stops what it started and exits with code 0

#### Scenario: A role whose parent is PID 1 runs on
- **WHEN** the development image starts, the supervisor at PID 1
- **THEN** the supervisor names process 1 to each role it starts, and every role reaches readiness and stays there

#### Scenario: A killed development container keeps every acknowledged commit
- **WHEN** a client commits through the development container until `docker kill` ends it, and the container starts again
- **THEN** every commit acknowledged before the kill reads back through the restarted container

### Requirement: Each role runs one runtime per core
Each role SHALL run one compio runtime per core it uses, a storage partition on its own and the system group on its own
in the instance hosting it, sharing no mutable state with another; what crosses between them SHALL go by message.
`[runtime] pin_cores` SHALL ask each runtime to bind to one CPU, chosen from those the process is permitted, as a
container's cpuset restricts them. Pinning SHALL be a performance choice and never a condition of correctness: where the
platform cannot bind a thread, as macOS cannot, offering only a cache-locality hint, the role SHALL run unpinned and say
so. Compute SHALL accept and serve clients on each of its runtimes, one per CPU it may use, on one client address bound
once: its readiness on the first, each runtime dialling its own connections to the system group and to the other roles,
and its counts of failed authentications held by its first runtime for all of them (client-endpoint, "Connections
authenticate against the metadata group's principals"). At start each role SHALL log, per runtime, the CPU it asked for
and what it has, read back from the operating system where it can be.

#### Scenario: A pinned role reports what it holds
- **WHEN** a role starts with `pin_cores = true` on a platform that allows binding
- **THEN** it logs each runtime's CPU, read back from the operating system, each within the permitted set

#### Scenario: A platform refusing to pin still serves
- **WHEN** a role starts with `pin_cores = true` where binding is unavailable
- **THEN** it logs that it runs unpinned, and serves requests

#### Scenario: Compute serves clients on every core
- **WHEN** 32 clients each commit a write over a connection of their own to a compute node of two partitions
- **THEN** every write is committed, compute logs a runtime per CPU, and more than one of its cores relays the writes

### Requirement: Readiness, health and shutdown
Each role SHALL serve `/health`, answering once it holds its settings (config-service) and has bound every listener it
serves from its start, so that `/health` answering says a connection to such an address is accepted: compute its client
address. Until then `/health` SHALL answer 503. Storage binds its partitions' addresses once they have recovered, with
`/ready`, so its `/health` answers while it recovers and a peer dialling it meanwhile is refused at once. Each role
SHALL also serve `/ready`, answering once its dependencies are reachable: storage once its partitions have recovered
their logs and joined, and in the hosting instance once the system group has recovered and listens before them
(system-group, "The hosting process starts the group first and stops it last"); compute once the system group answers
it, its view holds a placement and a close, Members lists every partition of the placement and a storage partition
answers. On SIGTERM compute SHALL announce itself draining (node-discovery, "A draining node is announced before it
stops"), stop accepting connections and finish its open requests, and storage SHALL apply and sync what its partitions'
logs hold, then stop the system group it hosts, before it exits.

#### Scenario: Storage is not ready before recovery
- **WHEN** a storage process restarts with 10,000 rounds to replay
- **THEN** `/health` answers once it holds its settings, and `/ready` answers only after the replay

#### Scenario: Compute is ready only once the books answer
- **WHEN** compute starts while the hosting storage process still recovers
- **THEN** compute's `/health` answers, and its `/ready` answers only after storage's

#### Scenario: Storage flushes on shutdown
- **WHEN** storage receives SIGTERM after acknowledging commits
- **THEN** every acknowledged commit is applied to its store before it exits, and a restart replays nothing

### Requirement: Configuration is explicit and validated at start
Each role SHALL take its settings from the config service, and the config service alone SHALL read a local settings
file, named by `--config`, holding its address, TLS files, key files, data directory and central file. The settings
SHALL be standard TOML parsed by `ultragraphdb_tool_toml` (settings-toml). Every table and setting name SHALL use only
`a`–`z`, `0`–`9` and `_`; a file naming any other SHALL be refused as an unknown setting. A role SHALL refuse to start
on an unknown setting, malformed settings or a missing required setting, naming the setting, and SHALL exit with code
78; the config service SHALL make every other check before it serves a revision (config-service). Each role's data SHALL
live under one directory per role and replica, laid out as on a cluster node.

#### Scenario: An unknown setting stops the start
- **WHEN** the storage binary receives settings whose `[storage]` table holds the key `partiton_count`
- **THEN** it exits with code 78 and an error naming `partiton_count`

#### Scenario: A missing required setting stops the start
- **WHEN** the storage binary receives settings with no `storage.data_dir`
- **THEN** it exits with code 78 and an error naming `storage.data_dir`

#### Scenario: A bad config-service file stops its start
- **WHEN** the config service starts with `--config` naming a file that does not exist
- **THEN** it exits with code 78 and an error naming the file

#### Scenario: The data layout matches a node's
- **WHEN** the development container has started
- **THEN** `/data` holds `system/meta/`, `system/seq/` and `storage/p1/r1/`, the paths a cluster's hosting storage node uses, and no directory of compute's

### Requirement: A role takes a port a killed process leaves held
Each role and the config service SHALL retry the bind of a TCP address refused with `EADDRINUSE` every 50 ms for up to 5
s, logging each retry and naming the address, and past that SHALL exit with code 69, the error naming the address and
what to do. Any other bind error SHALL end the start at once, as a Unix socket's refusal does: a role replaces a socket
left behind, so nothing frees such a path by waiting. A killed process is reaped before the kernel tears down the
io_uring ring it ran on, and an accept in flight on that ring holds its listener until then, so a role started at once
may find its port taken for some milliseconds.

#### Scenario: A port held a moment is taken
- **WHEN** a role starts at a TCP address another process holds, which lets it go within the bound
- **THEN** the role logs each retry naming the address, binds it, and answers `/health`

#### Scenario: Compute's /health answers once its listener is bound
- **WHEN** compute starts while another process holds its client address, which lets it go within the bound
- **THEN** `/health` answers 503 while the address is held, and once it answers 200 a client connection to the address is accepted at once

#### Scenario: A port held past the bound ends the start
- **WHEN** a role starts at a TCP address another process holds past the bound
- **THEN** it exits with code 69 after a retry logged per 50 ms of the bound, the error naming the address

#### Scenario: Any other bind error ends the start at once
- **WHEN** a role starts at a TCP address no interface of its host holds
- **THEN** it exits with code 69 at once, the error naming the address, with no retry logged

### Requirement: Roles authenticate each other with the cluster's role secret
Every role SHALL read the cluster's role secret at start from the file its `[cluster] role_secret_file` setting names, a
Kubernetes Secret on a cluster, and SHALL refuse to start with code 78 when the file is missing, unreadable or empty. A
role connection SHALL authenticate with SCRAM-SHA-256 against that secret before its first role command, each role
verifying its peer locally from the secret, since a role connection to the system group must open before the metadata
group's principals can be asked for. The role secret SHALL authenticate role connections only and SHALL be refused on
the client endpoint. In the development container each role's Unix socket SHALL be owned by user 65532 with mode 0600.

#### Scenario: A role with the wrong secret is refused
- **WHEN** a compute node started with a different role secret connects to the system group
- **THEN** its authentication is refused, it sends no role command, and it retries until its deadline

#### Scenario: The role secret opens no client session
- **WHEN** a client authenticates on the client endpoint with the role secret
- **THEN** authentication is refused with the same error as a wrong password

#### Scenario: A missing role secret stops the start
- **WHEN** a role starts with `role_secret_file` naming a file that does not exist
- **THEN** it exits with code 78 and an error naming the file
