# UltraGraphDB Single-Container Deployment

Nov 1, 2026 · @Marvin Hansen

This blueprint packages the complete three-node architecture (frontend, compute and storage, plus the metadata group and the sequencer) into one container for development. The distributed deployment is the normal case; the single container is a configuration of it, not a separate mode. Companion documents: [UltraGraphDB Distribution Architecture](https://claude.ai/artifact/W7YBsh2yPKCLjehZiEsFZD) and [UltraGraph Schema and UQL](https://claude.ai/artifact/LtXDHoVcin1aGFJWDxZDbL).

## Principle

The development container runs the cluster topology as the three role binaries (decided 2026-11-01, Shard-per-Core note P22), one process each, joined by the same transport the cluster uses over Unix-domain sockets. No role contains a branch for running alone; what changes is configuration: replica counts, the transport address and resource budgets.

Dgraph's standalone image is the reference and the cautionary tale: it runs Zero and Alpha as two processes started from a shell script inside one container, and its web UI was later dropped from the image and must be launched as a separate container. UltraGraphDB starts its three role processes from a supervisor binary, with no shell in the image, and keeps the web UI inside the frontend binary.

## Topology inside the container

&#91;embedded content: Development container · the three-node architecture as three processes\]

| Component | Cluster | Development container |
| --- | --- | --- |
| Metadata group | 3 or 5 frontend replicas | 1 replica, in the frontend process |
| Sequencer | Its own VSR group, 3 replicas | 1 replica, in the frontend process |
| Endpoint, Bolt adapter, web UI | On every frontend | One each, in the frontend process |
| Compute | Elastic pool | 1 compute process |
| Storage | Many groups of 3 replicas | 1 storage process with 2 groups of 1 replica by default; 2 groups of 3 in the dev-ha profile |

**Two storage groups by default.** With a single group, every write would be single-group and the cross-group machinery (change streams, Calvin ordering, epochs, watermarks) would never run on a developer's machine. Two groups make those paths run on every laptop, so bugs that would otherwise appear only in a cluster show up locally. A single-replica VSR group is valid; it simply has a quorum of one.

**The supervisor.** A small binary is the container's entrypoint: it starts the frontend, then storage, then compute, restarts none of them on its own, forwards signals, stops them in the shutdown order below, and exits with the first failing role's exit code, so the container's orchestrator sees a failure as it would in a cluster.

## Transport: one protocol, two carriers

Every role talks through a transport trait that carries the same byte-level protocol frames. The carrier is chosen by the address scheme in each node's membership record, so no code path ever asks whether it is running locally.

| Carrier | Address scheme | Used by | Notes |
| --- | --- | --- | --- |
| Network | `tcp://host:port` | Cluster deployments | io\_uring or epoll; TLS and authentication |
| Unix domain socket | `unix:///run/ultragraph/<role>.sock` | Development container (default) | Faster than loopback TCP; visible to standard tools |

Loopback TCP (`tcp://127.0.0.1`) also works, for example to capture traffic with standard tools.

**Parity rule.** Frames are fully encoded, bounds-checked and version-negotiated on every carrier, and every role crosses a process boundary as it does in a cluster. Only the movement of bytes differs, so protocol bugs cannot hide behind a local shortcut. TLS is off by default on the Unix-socket carrier; the handshake still runs.

## What is identical and what differs

| Identical to the cluster | Different in the container |
| --- | --- |
| Role binaries, protocol frames and version negotiation | Replica counts (one per group by default) |
| VSR, sequencer, epochs, change streams, watermarks | Transport carrier (Unix-domain sockets) |
| Schema compiler, write and query classification | I/O backend may fall back from io\_uring (below) |
| Storage engine and on-disk format | No hardened kernel or microVM |
| GQL, SPARQL, UQL and Bolt behaviour and conformance | TLS optional on the local carrier |
| Placement rules and coordination paths | Resource budgets sized to one machine; the supervisor starts the roles |

## I/O backend

Docker 25.0.0 and newer block `io_uring_setup`, `io_uring_enter` and `io_uring_register` in the default seccomp profile, because of vulnerabilities that can be exploited to escape containers. TigerBeetle, which has no fallback, fails to start under that profile. The development container therefore detects io\_uring at startup and falls back to compio's polling driver (epoll on Linux, kqueue on a native macOS build). The sans-IO core is unchanged; only throughput differs. For performance testing, a documented seccomp profile re-enables io\_uring for this container only; the cluster images ship that profile (D7).

## Resources

- **Cores.** Each role process sizes its thread-per-core runtime from its share of the container's CPU quota, set by the supervisor; with fewer cores than roles, the role processes share cores through the kernel's scheduler.
- **Memory.** The container's memory limit is split into per-role budgets (storage cache, compute working memory, frontend), each overridable.
- **Data volume.** One volume at `/data` with one directory per role and replica (`/data/meta`, `/data/seq`, `/data/storage/g1/r1` and so on), in exactly the layout a cluster node uses for the same role.

## Configuration

The container starts with no configuration: the supervisor's `dev` profile is the default command. Profiles are presets of the same settings the cluster uses, not separate modes.

| Profile | Topology | Carrier | Purpose |
| --- | --- | --- | --- |
| `dev` (default) | 2 storage groups of 1 replica | Unix-domain sockets | Everyday development; a group of one acknowledges from disk |
| `dev-ha` | 2 storage groups of 3 replicas | Unix-domain sockets | Failover testing: replicas can be stopped and restarted from the web UI. Where stage 8 builds VSR and runs the protocol note's X3 and X4 (`UltraGraphDB Distribution Protocol.md`) |
| `dev-net` | As `dev` | Loopback TCP | Protocol debugging and traffic capture |
| `frontend`, `compute`, `storage` | One role per container, each from its own image | Network | Cluster deployment |

| Port (proposed) | Purpose |
| --- | --- |
| 7687 | Bolt, for Neo4j drivers and tools |
| One native-protocol port | Binary protocol for native clients |
| One HTTP port | Web UI, admin API, health and metrics |

## Packaging

- **Images.** Each role ships its own image on the Wolfi glibc base (D7), running as a non-root user with no shell; web UI assets are compiled into the frontend binary. The development image layers the three role binaries and the supervisor on the same base.
- **Health endpoints:** `/health` for liveness and `/ready` once the metadata group, sequencer and storage groups are all up.
- **Schema on boot:** `.ugs` files mounted at `/schema` are compiled and applied at startup, so a project's schema lives in its repository.
- **Graceful shutdown:** on SIGTERM, frontends stop accepting work, compute drains, and storage groups flush, in that order; the supervisor stops the role processes in that order.

## From development to cluster

- The same schema file, queries and drivers work unchanged.
- Data moves by backup and restore or by export and import. The development container is not grown into a cluster, because replica counts and placement differ.
- An optional Compose file runs one container per role from the role images, for cluster-shaped testing on a single machine.

## Limits

The development container is not for production: replicas share one machine and one disk, the hardened kernel and microVM isolation are absent, and TLS is off on the local carrier by default.

## Open questions

- [ ] Should `dev-ha` replace `dev` as the default once the topology is stable?
- [ ] Native-protocol and HTTP port numbers.

## Sources

- [Docker: Seccomp security profiles](https://docs.docker.com/engine/security/seccomp/)
- [moby/moby #46762: block io\_uring syscalls in the default seccomp profile](https://github.com/moby/moby/pull/46762)
- [Dgraph: Basic cluster setup (standalone image)](https://www.dgraph.io/installation/single-host-setup)
- [Dgraph forum: standalone image no longer contains Ratel](https://discuss.dgraph.io/t/getting-started-docs-say-that-standalone-image-contains-ratel-but-it-doesnt/16767)
