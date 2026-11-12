# UltraGraphDB Architecture Review

Nov 12, 2026 · the service binaries under `../../../ultragraphdb` as built, against the core principles of `AGENTS.md` and the
distribution architecture of Oct 31, 2026

**Scope.** The review stays at the level of the binary: which binaries exist, what each one is for, who dials whom and
over what. Crates and dependencies are left out. Three texts are compared:

- `../../../AGENTS.md`: the three aims (Ultra Safe, Ultra Speed, Ultra Scale), their order (correctness, speed, scale, developer
  experience) and the six key properties of the design.
- The original design, `openspec/notes/UltraGraphDB Distribution Architecture.md`, the text of Oct 31.
- The amended design, `openspec/designs/UltraGraphDB Distribution Architecture.md`, the same note carrying the decisions
  taken since (D7, P22, P24). Where the amended copy already records a change, the review says so and counts it as
  settled.

## Summary

Six binaries exist where the original design named one. Four of them run in a cluster: the frontend, compute, storage
and the config service. The other two are a development supervisor and an administrator's command-line tool. The
three roles of the design are all there and their separation is real. The eight findings, most consequential first:

1. **Every commit asks one core of one process.** The frontend's owner holds the session registry, and each commit's
   owner check goes to it. The design says no commit waits on the metadata group.
2. **Storage is the heavy role, and the design's table says the opposite.** Storage executes transactions and runs the
   commit step. Both copies of the design still say storage runs narrow kernels and everything heavier runs on compute.
3. **Losing the frontend for 60 seconds stops the cluster.** Compute and storage both exit once the frontend stays
   away past the join deadline. The design's degraded modes have no path through the binaries.
4. **The config service and the supervisor are in no architecture document.** Every role needs the config service to
   start, and it is one instance.
5. **One shared secret admits every role, and role traffic is clear text.** The design's security by tier assumes
   credentials per tier.
6. **Three hardening statements of the design have no counterpart in a binary or the build:** the in-house DNS
   resolver, the io_uring operation allowlist and the libc symbol allowlist.
7. **The sequencer's placement is listed as an open question and is decided in code.**
8. **Document drift:** two tracked copies of the design that differ, seven citations of the superseded path, and a
   service-binaries rule in `../../../AGENTS.md` that one binary in the folder does not follow.

Findings 1 to 3 touch the aims of `../../../AGENTS.md`. Findings 4 to 8 are gaps between the documents and what runs.

## 1. The binaries as built

### Inventory

| Binary | Kind | State it holds | Image | Added |
| --- | --- | --- | --- | --- |
| `ultragraphdb_frontend` | Role | The metadata group and the sequencer, each a state machine over its own log (`/data/meta`, `/data/seq`), plus membership and the session registry | glibc on the Wolfi base | 2026-11-04 |
| `ultragraphdb_compute` | Role | None | glibc on the Wolfi base | 2026-11-04 |
| `ultragraphdb_storage` | Role | Per partition: a write-ahead log and a fjall store under `p<partition>/r0` | glibc on the Wolfi base | 2026-11-04 |
| `ultragraphdb_config` | Control-plane service | Every target's settings as immutable numbered revisions over its own log | static musl on scratch | 2026-11-04 |
| `ultragraphdb_supervisor` | Development entrypoint | None; it draws the secrets at the first start | the development image, with the four above | 2026-11-05 |
| `ultragraphdb_config_cli` | Administrator's tool | None | none | 2026-11-04 |

`ultragraphdb_frontend_profile` and `ultragraphdb_storage_profile` are the same two roles built with symbols and frame
pointers for a profiler. The benchmark and test helper binaries under `../../../ultragraphdb_tool`, `ultragraphdb_store/` and
`ultragraphdb_transactions/` are no part of the service architecture.

### What each binary does

- **Frontend.** The one door for clients and the one registry for roles. It recovers its owner, a single thread
  holding the metadata group, the sequencer, membership and the session registry. It then listens at two addresses:
  the role address, where compute and storage join, and the client endpoint, served by one endpoint core per CPU
  beside the owner's. It relays each client frame to the partition or the compute process that answers it, the body
  unchanged. It is ready once it has connected back to a partition.
- **Compute.** The read side. One runtime per core, no storage engine. It joins the frontend, learns from the member
  list where the partitions listen, and answers a read by scanning the partitions concerned and merging their rows in
  key order.
- **Storage.** The write side and the data. One thread and runtime per partition. Each partition fetches the placement
  map and the catalog versions from the frontend, replays its log, joins for itself, subscribes to the sequencer's
  epoch closes and leases its ID sequences. It executes transactions, runs the commit step, appends to its log,
  applies to its store, and answers compute's scans.
- **Config service.** The settings of every target, validated as a whole before they are stored or served. A role names
  its target and instance and fetches its settings at start. A role told to load a revision fetches it, checks it,
  shuts down and executes itself again.
- **Supervisor.** The development container's entrypoint. It starts the config service, the frontend, storage and
  compute in that order, waits on each over HTTP, forwards signals, stops them as frontend, compute, storage, config
  service, restarts nothing and exits with the first failure's code.
- **Config CLI.** Validates a settings folder exactly as the service would, before anything runs, and restores a
  folder from a running service.

### How they relate

```
                 clients
                    |  binary protocol, TLS 1.3 or clear text (7080)
                    v
   +-----------------------------------+        settings at start (HTTP, 8090)
   |             frontend              | ------------------------------+
   | owner: metadata group, sequencer, |                               |
   | membership, session registry      |                               v
   +-----------------------------------+                        +-------------+
      ^   |  relays reads       ^   |  relays writes,           |   config    |
 Join,|   |  (ComputeRead)      |   |  commits, one-frame       |   service   |
 Mem- |   v                     |   v  writes                   +-------------+
 bers +-----------+   Scan    +----------------------+             ^      ^
      |  compute  | --------> |       storage        |             |      |
      | (no store)| <-------- |  partition per core  | ------------+      |
      +-----------+  Entries  +----------------------+   settings         |
            |                   |   ^  Placement, Catalog, Join,          |
            |                   +---+  Subscribe, Lease, Report           |
            +------------------------- settings --------------------------+
```

### Who talks to whom

| From | To | Over | For |
| --- | --- | --- | --- |
| Client | Frontend | Binary protocol; TLS 1.3 or declared clear text; SCRAM against the metadata group's principals | Every request |
| Frontend | Storage partition | Binary protocol, role commands; one multiplexed connection per role per endpoint core | Relaying Bind, Write, Operation, Commit, Abort, DirectWrite, ConditionalWrite |
| Frontend | Compute | The same; compute processes taken in turn | Relaying reads as ComputeRead |
| Compute | Frontend | Binary protocol, role commands | Join, Members, and asking whether it still answers |
| Compute | Storage partition | Binary protocol, role commands | Scan, at one version per partition per read |
| Storage partition | Frontend | Binary protocol, role commands | Placement, catalog versions, Join, Subscribe to epoch closes, Lease of sequence blocks, Report, a request to close an epoch |
| Storage partition | Storage partition | Binary protocol between processes, messages inside one | Registering a reference at an entity's home, reverse reads fanned out |
| Each role | Config service | HTTP/1.1 with a read key | Its settings at start and at a load |
| Operator, supervisor, kubelet | Each role | HTTP/1.1 | `/health`, `/ready`, `/admin/load-config` with the admin key |
| Config CLI | Config service | HTTP/1.1 with the admin token | `restore` |

Three facts shape everything else:

- **The frontend is the hub.** Compute and storage each take one address, the frontend's, and learn the rest from it.
  No setting is a list. A cluster has exactly one frontend: a target's settings name one frontend file, and no
  frontend setting names a peer.
- **Two protocols, two credential systems.** The data plane and the role plane speak the binary protocol and
  authenticate with SCRAM-SHA-256, clients against principals and roles against one shared role secret. The control
  plane speaks an in-house HTTP/1.1 and authenticates with bearer keys.
- **Every durable state machine has one replica today.** The metadata group, the sequencer, each partition and the
  config service each sit behind the ordered-log interface that stage 8 puts VSR behind.

### How they run

| Shape | What runs | Carrier between roles |
| --- | --- | --- |
| Development container, profile `dev` | The supervisor and the four cluster binaries in one container | Unix-domain sockets |
| Development container, profile `dev-net` | The same | TCP on loopback |
| Helm chart, profile `kube` | StatefulSets of one replica for the config service, the frontend and storage; a Deployment of one replica for compute | TCP, clear text, labelled development only |

Start has a fixed dependency order: the config service, then the frontend, then storage and compute. Storage and
compute retry joining for the join deadline, 60 seconds by default.

## 2. Against the design of Oct 31

| Topic | Original design, Oct 31 | As built | Recorded |
| --- | --- | --- | --- |
| Number of binaries | One binary with role flags | Three role binaries, a config service, a supervisor, a CLI | Three binaries: P22, amended copy. The other three: stage 5.5's change design only |
| Frontend | Metadata VSR group, endpoint, Bolt adapter, planning, write classification, scheduling; scales by non-voting observers | One frontend. Endpoint, relay, metadata group, sequencer, membership and session registry in one process, the last four on one core | One replica until stage 8: amended copy. Bolt at stage 10.5: `../../../ROADMAP.md`. Observers: still in both copies |
| Compute | Query execution, graph algorithms, cross-group joins, aggregation; stateless; elastic | Quad-pattern reads: route, scan partitions, merge. Stateless. One replica in the chart | Query planning at stage 7: stage 5.5's design |
| Storage | Replica groups; applies change streams; runs only narrow kernels near the data | Executes transactions, validates, commits, logs, applies, builds indexes, registers references, answers scans | P22 states it. The architecture's table in both copies states the opposite |
| Placement | Predicate sharding | Bucket ranges per partition, one partition per core, one owner per fact | P24, amended copy |
| Derived data | Ordered change streams between groups, watermarks | None between partitions; referrer sets; epoch closes delivered by subscription | P24, amended copy |
| Cross-group writes | Classified by the schema; the remainder sequenced Calvin-style | Refused with `CrossPartition` | Stage 8: stage 5.5's design |
| Replication | VSR throughout | One replica behind the log's interface everywhere | Stage 8: amended copy |
| Sequencer | Its own VSR group on the frontends; merging with the metadata group an open question | Runs with the metadata group on one thread of the frontend, each over its own log | Still an open question in both copies |
| Internal traffic | The same binary protocol as clients | The binary protocol for roles; HTTP/1.1 for health, settings and loads | Stage 5.5's design only |
| Settings | Not addressed | A service of its own that every role needs to start | Stage 5.5's design only |
| Node runtime | Static musl everywhere; storage on a minimal kernel in a microVM | glibc with mimalloc on the Wolfi base; the config service static musl on scratch; a seccomp profile allowing io_uring | D7, amended copy. The amended copy's opening paragraph still says storage nodes boot a minimal hardened kernel |
| Role-to-role security | Streams between groups authenticated like client traffic; controls per tier | One shared role secret; clear text between roles | TLS and certificates per role at stage 8: stage 5.5's design |
| Formal model | TLA+ | Lean, `../../../lean/PlacementModel` and `lean/RoundModel` | Amended copy |
| Consistency levels | Snapshot and monotonic-fast | Both present in compute's reads | Aligned |

The pattern: the decisions that changed the data path were written back into the amended design. The decisions that
added processes and a control plane were not.

## 3. Review against the principles of AGENTS.md

### 3.1 Every commit asks one core of one process

**Principle.** Ultra Scale, and the key property "thread per core to minimize thread contention". The design adds its
own rule under *Keeping it small*: sequencer epochs exist "so no commit waits on the metadata group", and only
membership, the placement map and the versions need consensus.

**As built.** The session registry lives with the metadata group in the frontend's owner, on one core. Endpoint cores
hold no copy and ask the owner by message for each commit's owner check and each authentication's verifier. Stage
5.5's design states the reason: a stale copy would let a partition take an evicted session for a new one, which
breaks exactly-once. Correctness outranks speed, and the choice follows that order.

**Consequence.** The commit rate of the whole cluster passes through one core, however many partitions and storage
nodes are added. The archived change `batch-commits-through-the-roles` already had to let the owner answer checks
while it syncs, so the cost has shown in measurement once. At stage 8 the owner becomes a replicated group, and a
check that today is a message to another core becomes a question to that group's leader. That is the dependency the
design's own rule set out to avoid. The decision is recorded as a correctness choice. Its cost at scale is recorded
nowhere.

**Recommendation.** Before stage 8 is specified, decide where the session registry lives once the metadata group has
three replicas, and state in the design that commits depend on it.

### 3.2 Storage is the heavy role, and the design's table says the opposite

**Principle.** The key property "storage compute separation". The design gives it a direction: storage runs "only
narrow kernels near the data", "everything heavier runs on compute", so that the storage image stays minimal and
graph work does not compete with storage I/O.

**As built.** The separation of processes is real, and a test in each of the frontend's and compute's suites proves
that neither links a storage engine. The direction is reversed for writes. Storage executes transactions, runs
commit-time operations, validates, commits, builds indexes and registers references. Compute merges scans. P22 says
so in one line: storage is "transactions, the commit step, the log, the store".

**Consequence.** Three things follow, and none is written down.

- The architecture's node table contradicts P22 in both copies of the design. A reader of the table expects a thin
  storage node.
- The first reason for thin storage, a minimal hardened image, went with D7. The second, keeping heavy work off the
  storage node's I/O, now holds for reads alone.
- The design's S3 mode shrinks storage to a log tier holding the log and recent deltas, with tiles in S3 and the tile
  cache on compute. A role that executes transactions and decides conditional writes reads the data it decides on.
  The two statements meet when S3 mode is designed.

**Recommendation.** Rewrite the storage and compute rows of the design's node table to what P22 decided. Take the
S3-mode question into the tile store's design, which is open now.

### 3.3 Losing the frontend for 60 seconds stops the cluster

**Principle.** Ultra Safe: run correctly, or shut down on detecting that an expectation was violated.

**As built.** Compute asks the frontend and every partition whether they still answer, and exits with "unavailable"
when one stays away past the join deadline. A storage partition ends its core when the frontend stays away past the
same deadline. The supervisor restarts nothing.

**Consequence.** A lost peer is an availability event, and the binaries treat it as a violated expectation. With one
frontend, a frontend outage longer than 60 seconds ends every other role. Under Kubernetes the pods restart, and each
restart needs the config service. In the development container the cluster stays down. The design promises more:
during a sequencer view change, single-group writes, single-group reads and monotonic-fast reads continue. A partition
does keep stamping its open epoch while the sequencer is away. No client can reach it meanwhile, because every client
frame passes through the one frontend. Stage 8's several frontends restore that path. Whether storage should outlive
the frontend group is undecided.

**Recommendation.** State in the design what each role does while the frontend group is unreachable, and for how
long, before stage 8 builds the group.

### 3.4 The config service and the supervisor are in no architecture document

**Principle.** `../../../AGENTS.md`, *Read before you write*: land new code at the right level of abstraction, and flag an
inconsistency of abstraction. The architecture note is the level at which a process of the cluster is introduced.

**As built.** The config service is a fourth stateful process with its own log, its own cluster identity, its own two
keys, its own protocol and its own image base. Every role needs it to start, including a role restarting after a
crash, and it is one instance. Stage 5.5's design accepts that until stage 8 and says why. Neither copy of the
distribution architecture mentions the service or the supervisor.

**Consequence.** The document that defines the cluster describes three node types, and a running cluster has four
kinds of process. The upgrade order, the security tiers and the failure discussion of the design all leave the fourth
out. Stage 8 will replicate it through the same log interface, which makes it a fourth kind of replica group beside
the metadata group, the sequencer and the partitions.

**Recommendation.** Add the config service to the design's node table, its upgrade order and its security tiers. Name
the supervisor as the development entrypoint in the runtime section.

### 3.5 One shared secret admits every role, and role traffic is clear text

**Principle.** Ultra Safe, defense in depth. The design's security by tier gives storage, compute and the frontend
different credentials, and states that a compromised node cannot inject updates into another group.

**As built.** Every role reads the same role secret. A Join is accepted from any holder of it, and a later Join at an
address replaces the member that held the address. The secret therefore proves membership of the cluster and says
nothing of which role is speaking. Role connections carry no TLS on TCP. The `kube` profile also serves clients in
clear text and says it is for development only.

**Consequence.** One compromised compute process holds the credential to join as a storage partition. The design's
statement about injected updates does not hold for the binaries as built. Stage 5.5's design defers TLS between roles
and certificates per role to stage 8, so the gap is known. The Helm chart is the first artifact that puts role traffic
on a network, and it carries the gap with it.

**Recommendation.** Keep the stage 8 plan. Until then, state on the chart itself, not only in the profile's comment,
that it is no production deployment.

### 3.6 Three hardening statements have no counterpart

**Principle.** Ultra Safe, and the rule that documentation states what is.

**As built.**

- The design says a pure-Rust resolver is used and "the libc resolver is never called". The repository holds no
  resolver. The `kube` profile's role addresses are hostnames, handed to the runtime's TCP connect.
- The design restricts io_uring to the operations compio uses. The shipped seccomp profile allows the three io_uring
  system calls, and nothing registers a restriction on the ring.
- The design has continuous integration fail a release that links an unapproved libc symbol. No continuous
  integration exists yet, and the roles now link glibc dynamically.

**Consequence.** The first is the only one that changes behaviour today: name resolution in a cluster depends on the
base image's resolver. The amended design files the microVM and its kernel under tracked options, and these three
statements stand outside that filing as present fact.

**Recommendation.** Move each of the three under the tracked options, or give each a stage.

### 3.7 The sequencer's placement is decided in code and open on paper

The design asks whether the sequencer is its own VSR group or merges into the metadata group for small clusters. The
frontend runs both on one thread, each over its own log, and the metadata group learns each catalog version's
effective epoch from the sequencer's close. That coupling is a decision. The open-questions list of both copies still
carries the question.

### 3.8 Document drift

- **Two tracked copies that differ.** `..` holds the amended design. `openspec/notes/` holds the
  original text as a staged addition. They differ in 26 places.
- **Seven citations name the notes path,** among them `../../../ROADMAP.md`, `SCHEMA.md`, `bazel/README_BAZEL.md` and
  `backlog.md`. If the staged copy is committed where it is, those citations resolve to the superseded text.
- **The service-binaries rule of `../../../AGENTS.md` lists five binaries, each of `main.rs` and `service.rs`.** The folder
  holds a sixth, `ultragraphdb_config_cli`, made of `main.rs` and `restore.rs`. `../../../AGENTS.md` reserves that folder for
  service binaries and their test crates and gives tools the folder `ultragraphdb_tool/`.
- **`../../../AGENTS.md` names `ultragraphdb_storage_profile` and leaves out `ultragraphdb_frontend_profile`.**
- **The amended design's node table still gives the frontend a Bolt adapter and observer frontends.** The roadmap
  places Bolt at stage 10.5, and no stage builds observers.

## 4. Where the binaries follow the principles

- **Zero copy on the wire.** The frontend relays a frame with its body unchanged, and roles speak the client's
  protocol.
- **Thread per core.** An endpoint core per CPU in the frontend, a runtime per core in compute, a runtime per
  partition in storage, each owning its listener.
- **Storage and compute as separate processes,** enforced by a test and not by convention.
- **VSR built up from one replica behind the log's interface.** The metadata group, the sequencer, the partitions and
  the config service all sit behind it, and a partition's directory already names its replica.
- **Fail-stop.** A role stops with a `sysexits` code that names the cause, and the supervisor hides no failure.
- **Clear text is always a visible choice.** An endpoint naming neither a certificate nor `plaintext = true` stops the
  start.
- **Typed requests.** Read-only, read-write, DirectWrite and ConditionalWrite are distinct commands, which is the
  ground the schema-driven classification of stage 8 stands on.
- **Both consistency levels** of the design exist in compute's reads.

## Open questions

- [ ] Where does the session registry live once the metadata group has three replicas, and does a commit then wait on
  that group's leader?
- [ ] Does the frontend stay the relay of every client frame at stage 10, or do clients reach partitions directly
  with the session token the design describes?
- [ ] What does a storage partition do while the frontend group is unreachable: stop after the join deadline, as
  today, or hold its data open for longer?
- [ ] In S3 mode, which role executes transactions when storage is a log tier and the tile cache sits on compute?
- [ ] Is the config service a replica group of its own at stage 8, or does it move into the metadata group?
- [ ] Is the sequencer's coupling to the metadata group in one owner the decision for every cluster size?
- [ ] Does `ultragraphdb_config_cli` belong under `../../../ultragraphdb_tool`, or does the rule of `AGENTS.md` widen to
  admit it?
- [ ] Which of the two copies of the distribution architecture stays, and where does the original go?
