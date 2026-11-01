# The node

The node is everything above the I/O boundary of stage 5: the process, its threads and runtimes, its configuration,
its data directory, its start and shutdown, its health endpoint, its acceptance tests, and the verification and
measurements that need the running binary. It was lifted out of
`openspec/changes/stage-5-transactions-and-binary-protocol` on 2026-11-01, to be specified later; stage 5 keeps the
I/O boundary and everything below it: the session layer (`ultragraphdb_server_session`, sans-I/O), the pattern engine,
the commit step's work and the log. The text below is what that change held, moved verbatim.

## Requirements (from the change's `server-node` spec)

### Requirement: Cores are divided between workers and the commit step
The server SHALL run the commit step on one core and a worker on each of the remaining cores available to the process,
each worker a compio runtime pinned to its core; with one core available, one worker SHALL share it with the commit
step. A connection SHALL be accepted onto the least-loaded worker and stay
there, and an interactive transaction SHALL run on its connection's worker.

#### Scenario: A node with eight cores runs seven workers
- **WHEN** the server starts with eight cores available
- **THEN** it runs seven workers and one commit step

#### Scenario: A node with one core still serves
- **WHEN** the server starts with one core available
- **THEN** it runs one worker and the commit step on that core, and accepts connections

### Requirement: Queues are bounded
Every queue between workers and the commit step SHALL be bounded; a commit arriving at a full queue SHALL be refused as
busy rather than queued.

#### Scenario: Load beyond the queue is refused, not buffered
- **WHEN** commits arrive faster than the commit step decides them and its queue is full
- **THEN** further commits are refused as busy until the queue drains

### Requirement: Configuration
The server binary SHALL read its configuration once at start, from its environment: the listen and health addresses, the
data directory, whether connections authenticate and the bootstrap secret file, the transaction limits, the
power-loss-protection flag, and overrides of the round bounds and the checkpoint interval. The cluster identity SHALL be
drawn at random when the data directory is created and kept in the superblock, not configured. Library crates SHALL take
their settings as values and read no environment.

#### Scenario: An invalid setting stops the start
- **WHEN** the server starts with a read-write limit of zero
- **THEN** it exits with code 2 and an error naming the setting, before opening the store

### Requirement: Start, recovery and shutdown
The server SHALL open the store and the log, run recovery, and only then accept connections; a recovery that fails SHALL
stop the start with its error. On shutdown, at SIGTERM or SIGINT, it SHALL stop accepting, answer requests with Error
17, abort open transactions, decide and answer the commits already queued within its shutdown grace, sync the log and
the store, and exit with code 0. Telemetry SHALL be installed once, through `ultragraphdb_common_telemetry`.

#### Scenario: Connections wait for recovery
- **WHEN** the server starts with rounds to replay
- **THEN** no connection is accepted until every round is replayed

#### Scenario: A shutdown loses no acknowledged commit
- **WHEN** the server is shut down while commits are in flight and then restarted
- **THEN** each commit in flight was either acknowledged and is present, or aborted and is absent

### Requirement: The server authenticates its connections
A server requiring authentication SHALL hold the bootstrap administrator from the secret file its configuration names,
read once when it creates the cluster and kept as its SCRAM-SHA-256 verifier alone, and SHALL create no cluster without
that file. It SHALL refuse a password shorter than 15 characters or on the operator's list of common or breached
passwords. Failed authentications SHALL be throttled per principal and per source address with a growing delay, and
after 100 consecutive failures of a principal its authentications SHALL be refused from every source for the refusal
interval, 15 minutes by default, after which they are accepted again; a success SHALL reset the count. A server
configured without authentication SHALL listen on the loopback interface only, and SHALL refuse to start on any other
address. Each authentication, its outcome and the principal it named SHALL be written to the audit log, and no password,
proof or token SHALL be written to any log (`ultragraphdb_protocol/SECURITY.md`).

#### Scenario: No bootstrap secret, no cluster
- **WHEN** a server requiring authentication starts on an empty data directory without the bootstrap secret file
- **THEN** it exits with an error naming the missing file, and creates no cluster

#### Scenario: A short bootstrap password creates no cluster
- **WHEN** a server requiring authentication starts on an empty data directory with a bootstrap secret file holding a
  password of 14 characters
- **THEN** it exits with an error naming the password rule and not the password, and creates no cluster

#### Scenario: The bootstrap administrator authenticates
- **WHEN** a server requiring authentication creates its cluster from a bootstrap secret file, and a client
  authenticates as the bootstrap administrator with the file's password
- **THEN** the exchange succeeds, and the data directory holds the password's verifier and not the password

#### Scenario: Repeated failures refuse a principal for the interval
- **WHEN** a principal's authentication fails 100 times in a row, and the principal then authenticates with its
  password at once and again after the refusal interval
- **THEN** the first is refused with code 19 and the second succeeds

#### Scenario: A server without authentication listens on loopback only
- **WHEN** a server configured without authentication is given a listen address that is not loopback
- **THEN** it exits with an error naming the setting, before opening the store

#### Scenario: No secret reaches a log
- **WHEN** a client authenticates once with a wrong password and once with the right one
- **THEN** the audit log holds both attempts with their outcomes and the principal named, and no log holds the
  password, a proof or a SCRAM message

### Requirement: The node reports its health
The server SHALL serve HTTP/1.1 on its health address, apart from its protocol address: `GET /livez` SHALL answer 200
while the commit step and every worker run their loops, recovery included, and 503 once one has stalled for 10 seconds;
`GET /readyz` SHALL answer 200 from when the node accepts connections until its shutdown begins or it stops on a
failure, and 503 otherwise. Every other path SHALL answer 404. The endpoint SHALL hold no data and require no
authentication.

#### Scenario: Readiness waits for recovery
- **WHEN** the server starts with rounds to replay and `/readyz` is asked during the replay
- **THEN** it answers 503, `/livez` answers 200, and `/readyz` answers 200 once connections are accepted

#### Scenario: Readiness ends with the shutdown
- **WHEN** the server receives SIGTERM
- **THEN** `/readyz` answers 503 before the listener closes

### Requirement: Connections are bounded
The server SHALL hold at most its configured number of connections; a connection past it SHALL have its Hello answered
with Error 5 (Busy) on stream 0 and be closed.

#### Scenario: A connection past the limit is told why
- **WHEN** a server configured for two connections holds two and a third connects
- **THEN** the third's Hello is answered with Error 5 and the connection closes, and the first two are served

### Requirement: A store or log failure stops the node
A failed sync, a store or log I/O failure, a corruption found, or a violated invariant SHALL stop the node: every
request from then on SHALL be answered with Error 17 (Unavailable), the failure SHALL be logged at error naming it, and
the node SHALL shut down and exit with code 1, so recovery starts from the log at the next start.

#### Scenario: A failed sync stops the node
- **WHEN** the log's sync fails while commits are in flight
- **THEN** none of them is acknowledged, later requests are answered with Error 17, and the process exits with code 1

### Requirement: Kill tests on the binary
The server binary SHALL be killed during rounds, syncs and checkpoints and restarted; after each restart every
acknowledged commit SHALL be present, and every round SHALL be present whole or absent whole. A commit killed before its
reply MAY be present, since its round may have been synced; its client resolves it through its session.

#### Scenario: A kill during a sync
- **WHEN** the server is killed while a round's sync is in progress and restarted
- **THEN** the round's transactions are all present or all absent, none of them was acknowledged, and each client's
  retry through its session learns which

### From the requirement "History checkers" (`transaction-verification`)

Elle's list-append and read-write register workloads SHALL run through the Rust client against the server, with
real-time edges, and SHALL find no anomaly.

#### Scenario: Elle finds no anomaly
- **WHEN** Elle's list-append workload runs at 8 concurrent clients against the server
- **THEN** Elle reports the history strictly serializable

## Design (from the change's `design.md`, *The server*)

#### Crates

| Crate | Folder | Depends on | Holds |
|---|---|---|---|
| `ultragraphdb` | `ultragraphdb_server/` | `ultragraphdb_server_session`, `ultragraphdb_common_telemetry`, `ultragraphdb_store_kv` | The server: its library and the `ultragraphdb` binary (renamed from `ultragraphdb_server`, 2026-10-30) |

| Crate | Holds |
|---|---|
| `ultragraphdb` | The node: configuration, the data directory, cluster creation, recovery, the threads and their runtimes, the acceptor, the commit-step thread, the health endpoint, signals and shutdown, telemetry; the `ultragraphdb` binary |
| `ultragraphdb_tests` | The acceptance batteries: the built binary started by `service_utils` and driven through `ultragraphdb_client_tokio`, as scrith's `smdb_tests` drive their services |

#### Threads and queues

A node runs, inside one `std::thread::scope` so every thread borrows the store, the applier and the configuration
without reference counting:

- **The commit-step thread**, a compio runtime owning the `Pipeline` (its log, its step, its applier's apply).
- **Workers**, one compio runtime per thread, one thread per core but the commit step's (n−1, at least one; on one core
  the worker and the commit step share it, `server-node`), each pinned with `sched_setaffinity` (`rustix`) on Linux and
  unpinned on macOS. A worker owns its connections, their transactions and their statement caches.
- **The acceptor**, a thread with a blocking `std::net` listener: it accepts, sets `TCP_NODELAY` and keepalive (60 s
  idle, 10 s interval, 6 probes), and hands the socket to the worker holding the fewest connections (an `AtomicU32` per
  worker; ties to the lowest index) over that worker's bounded channel.
- **The health endpoint**, a thread of its own (*Health*).

Every compio runtime is built with `single_issuer` and `defer_taskrun` on Linux (decided 2026-10-27); the node checks
at start that the kernel is 6.6 or later and refuses to start below it, naming the kernel. The node's threads are its
own: `std::thread::scope` carries the `#[expect(clippy::disallowed_methods, reason)]` the store's production
implementations carry (`clippy.toml`), and the simulator drives the step functions those threads run, not the threads.

**Worker to commit step.** One bounded channel (`futures-channel`, as decided) carries the pipeline's inputs —
commits, registrations, drops, retries — tagged with a ticket: the worker's index in its top 8 bits, a counter of the
worker's in the rest. A send to a full channel is refused at once with code 5 (Busy). The pipeline's own waiting
queue is bounded by `Pipeline::with_queue`. **Commit step to worker:** one bounded channel per worker carries replies
and the transaction buffers coming back (`Pipeline::buffers_mut`), routed by the ticket's top bits; its capacity is the
most commits a worker may have in flight, so the commit step never waits on it, and a full one is an invariant
violation. A worker maps each ticket to its connection and stream. **Read floor:** each worker publishes the read
version of its oldest open read-write transaction in an `AtomicU64`; the commit step takes their minimum, or the
watermark when none is open, as `Input::OldestRead` once per round.

**Time.** Every thread reads the time through the clock it is given (`clippy.toml`'s `disallowed-methods`). A worker
checks a transaction's limit (`TxnLimits`, the Welcome's `transaction_ms`) at each of its requests and once a second
over all its open transactions, ending an expired one with Error 4 on its stream.

#### Bounds

| Bound | Default | Past it |
|---|---|---|
| Connections per node | 1,024 | the Hello is answered with Error 5 (Busy) on stream 0, and the connection closes |
| Frame buffers per worker | 64 of `max_frame` | a connection's next frame waits for a free buffer |
| Commits in flight per worker | the commit queue divided among the workers | Error 5 |
| Commit queue (channel and pipeline) | 4,096 | Error 5 |

Outgoing frames are built into the worker's pooled buffers, so a worker's steady state allocates nothing (Ultra
rule 3).

#### Configuration

The node reads its configuration once, at start, from environment variables, as scrith's `dgraph_migration` does:
every place it runs supplies the environment, a container manifest and the test harness alike. The binary takes no
arguments but `--help` and `--version`. Every value is parsed and range-checked before the data directory is opened;
the first one broken stops the start with exit code 2, naming the variable and its rule.

| Variable | Default | Rule |
|---|---|---|
| `UGDB_DATA_DIR` | required | a directory, created when missing |
| `UGDB_LISTEN` | `127.0.0.1:7400` | an address and port; loopback when `UGDB_AUTH` is `off` |
| `UGDB_HEALTH_LISTEN` | `127.0.0.1:7401` | an address and port, apart from `UGDB_LISTEN`'s |
| `UGDB_AUTH` | `on` | `on` or `off` |
| `UGDB_BOOTSTRAP_SECRET_FILE` | none | required with `UGDB_AUTH=on` on an empty data directory |
| `UGDB_PASSWORD_DENYLIST_FILE` | none | a file of passwords, one per line |
| `UGDB_AUDIT_LOG` | `<UGDB_DATA_DIR>/audit.log` | a file path |
| `UGDB_WORKERS` | cores − 1, at least 1 | 1 to cores |
| `UGDB_CONNECTIONS_MAX` | 1,024 | 1 to 65,535 |
| `UGDB_FRAME_BUFFERS` | 64 | per worker, at least 4 |
| `UGDB_COMMIT_QUEUE` | 4,096 | 1 to 1,048,576 |
| `UGDB_TRANSACTION_MS` | 10,000 | a read-write transaction's life, 1 to 600,000 |
| `UGDB_READ_ONLY_MS` | 16,000 | 1 to 3,600,000 |
| `UGDB_MAX_WRITE_SET` | 1,040,384 | 4,096 to 1,040,384 |
| `UGDB_SESSIONS`, `UGDB_STATEMENTS`, `UGDB_MAX_STREAMS`, `UGDB_INITIAL_CREDIT`, `UGDB_MAX_FRAME` | §11.9's | §11.9's ranges |
| `UGDB_IDLE_LIMIT_S`, `UGDB_AUTH_TIMEOUT_S`, `UGDB_REFUSAL_INTERVAL_S` | 600, 60, 900 | at least 1 |
| `UGDB_POWER_LOSS_PROTECTION` | `false` | `true` or `false` |
| `UGDB_ROUND_BYTES`, `UGDB_ROUND_TRANSACTIONS`, `UGDB_CHECKPOINT_BYTES` | the flag's (`WalConfig`) | within the log's limits |
| `UGDB_SHUTDOWN_GRACE_S` | 30 | 0 to 3,600 |
| `UGDB_LOG_FILTER`, `UGDB_LOG_FORMAT`, `UGDB_OTLP_ENDPOINT` | `info`, `plain`, none | `ultragraphdb_common_telemetry`'s |

#### The data directory and the cluster

`<UGDB_DATA_DIR>/store` holds the store (fjall), `<UGDB_DATA_DIR>/wal` the log, `audit.log` the audit log by default. At
start the node reads the log's superblock (`SuperblockRead`): `Present` gives the cluster; `Empty` on an empty data
directory creates one, the cluster drawn from the operating system's randomness; `Empty` beside a store, or
`Unreadable`, stops the start naming the directory (exit 3).

**The bootstrap administrator.** A cluster created with authentication takes its first principal from the bootstrap
secret file: UTF-8, the password alone, one trailing line feed (or CR LF) dropped. The node refuses a file readable by
group or others on Unix, a password under 15 characters or on the deny list, naming the rule and never the password,
and creates nothing. It derives the SCRAM-SHA-256 verifier at `DEFAULT_ITERATIONS` and proposes the principal to the
commit step as its first input, `Input::CreatePrincipal { name, verifier }`, numbered by its commit version as a session
is: its round writes the principal's entry under `\xffultragraphdb/security/principal/` without versions, the security
catalog's first entry and its format's first version, and its write-set record names it with operation 8, create a
principal (`PROTOCOL.md` §12.2, to be added), so replay restores it. Until stage 7 the node loads every principal entry
at start into the authenticator's `Credentials`; the file is read only when the cluster is created.

#### Start and shutdown

Start: configuration; telemetry, once; the kernel check; the data directory; the log opened and recovered
(`Wal::open`); the store opened (`Durability::Process`: the log is the durable record); `Replay::run`; the pipeline;
the principal entries; the threads; the listener bound; then the health endpoint turns ready and the node logs
`ultragraphdb ready` with both addresses.

Shutdown, on SIGTERM or SIGINT: readiness turns false; the listener closes; requests on open connections are answered
Error 17; open transactions are aborted; the commits already queued are decided, synced and answered for up to
`UGDB_SHUTDOWN_GRACE_S`; the log and the store are synced; the node exits 0. A second signal exits at once, code 1;
recovery covers it. A fail-stop takes the same path with exit code 1.

| Exit code | Meaning |
|---|---|
| 0 | a clean shutdown |
| 1 | a fail-stop, a failed sync, a second signal |
| 2 | a configuration refused, or a kernel below 6.6 |
| 3 | a data directory or a recovery refused |

#### Health

A thread of its own serves HTTP/1.1 on `UGDB_HEALTH_LISTEN`, after Kubernetes' probe conventions and the paths
`kube-apiserver` serves: `GET /livez` and `GET /readyz`, 200 with `ok` or 503 with the reason as text, every other path
404, every request one response and the connection closed. It reads a request line and headers of at most 1 KiB within
one second, answers one request at a time, and holds no data: it reads the node's state from atomics.

- `/livez` answers 200 while the commit-step thread and every worker have marked their loop within the last 10 seconds,
  through recovery included, so a long recovery is not restarted; a stalled loop answers 503.
- `/readyz` answers 200 from the ready log line until shutdown begins or a fail-stop, 503 before and after: Kubernetes
  sends connections only to a node that accepts them.

Kubernetes probes `/livez` for liveness, `/readyz` for readiness and as the startup probe. The tests wait on `/readyz`.
It is written over `std::net` by hand, a few hundred lines, so the node adds no HTTP or gRPC stack (tonic and hyper
need tokio; the node runs compio).

#### Tests

- **Unit and BDD**, in each crate: configuration, the start's decisions and the health responses as step functions,
  without a runtime.
- **Acceptance**, `ultragraphdb_tests`: the binary copied into the sandbox (`copy_file`), started by
  `service_utils::ServiceUtil` with its environment (a data directory under `TEST_TMPDIR`, a port per battery),
  `WaitForHttpHealthCheck` on `/readyz`, then a battery of scenarios sequenced in one `#[test]` through
  `ultragraphdb_client_tokio`, as scrith's `smdb_tests` run. Every server scenario and the conformance suite run this
  way (10.5). `service_utils` joins the workspace's dependencies for it, as in scrith.

#### Risk

- [Elle runs on the Java virtual machine] → it runs outside Bazel, as the Lean model does, through the Rust client.

#### Open Questions

2. Threads: one `std::thread::scope`, a blocking acceptor thread, pinning through `rustix`, tickets carrying the
   worker's index.
3. The bounds' defaults: 1,024 connections, 64 frame buffers per worker, a commit queue of 4,096.
5. Configuration from `UGDB_*` environment variables, the ports 7400 and 7401, and the exit codes.
7. The health endpoint: HTTP/1.1 written over `std::net`, `/livez` and `/readyz`.

## Tasks (from the change's `tasks.md`)

- [ ] 10.1 Scenarios: server-node, all requirements, the health endpoint, the connection bound, fail-stop and reads
  among them; wire-protocol ("The Rust client", end to end against the node), transactions ("A read above the
  watermark is refused", at the session); the two scenarios group 2 left to the server, "A refused request keeps the
  connection" and "A large result streams", with the cluster and unknown-command scenarios rebound to answering and
  closing the connection; vector-index, the dimension declared on the wire and the contradiction refused
- [ ] 10.5 Phases 1–3, the node (`ultragraphdb`): configuration from `UGDB_*`, every value checked before the data
  directory opens; the data directory, the superblock's three outcomes and cluster creation with the bootstrap
  administrator (the secret file's rules); the start's order, the shutdown's and the exit codes; the health endpoint's
  responses; the bounds; the ticket routing between workers and the commit step
- [ ] 10.6 Phase 4: 10.2 to 10.5 implemented against their audited suites: the threads in one scope, workers pinned on
  Linux, the commit step on its thread, the acceptor handing sockets to the least-loaded worker, bounded channels
  answering busy, every compio runtime with `single_issuer` and `defer_taskrun` (decided 2026-10-27) and the kernel
  checked for 6.6; the health endpoint over `std::net`; telemetry through `ultragraphdb_common_telemetry`, the audit
  target written to its own file; signals and the shutdown's grace; fail-stop
- [ ] 10.7 Acceptance (`ultragraphdb_tests`): the binary copied into the sandbox and started by `service_utils`, its
  environment set per battery, `WaitForHttpHealthCheck` on `/readyz`, batteries sequenced in one `#[test]` each through
  `ultragraphdb_client_tokio`, as scrith's `smdb_tests` run; every server scenario and the conformance suite;
  `service_utils` pinned in the workspace and listed in `AGENTS.md`
- [ ] 10.8 Phase 5: fuzz targets for the decoders the group adds — resume keys, the configuration's values, the health
  endpoint's request, the bootstrap secret file, the principal entry — built and not run (11.7); mutation testing is
  11.8's, at the stage's close
- [ ] 11.5 Elle's list-append and read-write register workloads through the client at 8 clients, with real-time edges, outside Bazel; the runs and their results recorded
- [ ] 11.6 Kill tests on the binary during rounds, syncs and checkpoints
- [ ] 11.9 The write-ahead log's compio suite and kill tests on Linux, in an OrbStack container, so the `O_DIRECT` path, which only type-checks on macOS, runs against a real Linux file system (stage 5 keeps the compio suite; the node takes the kill tests)
- [ ] 12.2 Throughput at 1, 2, 4, 8 and n−1 workers with abort rates; the commit step's cost per transaction against the execution cost divided by n−1
- [ ] 12.7 Commit latency (median and p99) and throughput at n−1 workers with the runtimes' `single_issuer` and `defer_taskrun` on and off, on Linux
