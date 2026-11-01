# UltraGraphDB Single-Container Deployment

Nov 1, 2026 · @Marvin Hansen

This blueprint packages the complete three-node architecture (frontend, compute and storage, plus the metadata group and the sequencer) into one container for development. The distributed deployment is the normal case; the single container is a configuration of it, not a separate mode. Companion documents: [UltraGraphDB Distribution Architecture](https://claude.ai/artifact/W7YBsh2yPKCLjehZiEsFZD) and [UltraGraph Schema and UQL](https://claude.ai/artifact/LtXDHoVcin1aGFJWDxZDbL).

## Principle

The development container runs the cluster topology folded into one process. No role contains a branch for running alone; what changes is configuration: replica counts, the transport carrier and resource budgets.

Dgraph's standalone image is the reference and the cautionary tale: it runs Zero and Alpha as two processes started from a shell script inside one container, and its web UI was later dropped from the image and must be launched as a separate container. UltraGraphDB runs every role in one process with the web UI built in.

## Topology inside the container

&#91;embedded content: Development container · the three-node architecture in one process\]

| Component | Cluster | Development container |
| --- | --- | --- |
| Metadata group | 3 or 5 frontend replicas | 1 replica |
| Sequencer | Its own VSR group, 3 replicas | 1 replica |
| Endpoint, Bolt adapter, web UI | On every frontend | One each |
| Compute | Elastic pool | 1 instance |
| Storage | Many groups of 3 replicas | 2 groups of 1 replica by default; 2 groups of 3 in the dev-ha profile |

**Two storage groups by default.** With a single group, every write would be single-group and the cross-group machinery (change streams, Calvin ordering, epochs, watermarks) would never run on a developer's machine. Two groups make those paths run on every laptop, so bugs that would otherwise appear only in a cluster show up locally. A single-replica VSR group is valid; it simply has a quorum of one.

## Transport: one protocol, three carriers

Every role talks through a transport trait that carries the same byte-level protocol frames. The carrier is chosen by the address scheme in each node's membership record, so no code path ever asks whether it is running locally.

| Carrier | Address scheme | Used by | Notes |
| --- | --- | --- | --- |
| Network | `tcp://host:port` | Cluster deployments | io\_uring or epoll; TLS and authentication |
| Unix domain socket | `unix:///run/ultragraph/<role>.sock` | Multi-process local runs | Faster than loopback TCP; visible to standard tools |
| In-process | `inproc://<role>/<id>` | Development container (default) | Frames passed as buffers between role runtimes; no copy, no system call |

Loopback TCP (`tcp://127.0.0.1`) also works, for example to capture traffic with standard tools.

**Parity rule.** Frames are fully encoded, bounds-checked and version-negotiated on every carrier, including in-process. Only the movement of bytes differs, so protocol bugs cannot hide behind the local carrier. TLS is off by default on the in-process and Unix-socket carriers; the handshake still runs.

## What is identical and what differs

| Identical to the cluster | Different in the container |
| --- | --- |
| Binary, protocol frames and version negotiation | Replica counts (one per group by default) |
| VSR, sequencer, epochs, change streams, watermarks | Transport carrier (in-process) |
| Schema compiler, write and query classification | I/O backend may fall back from io\_uring (below) |
| Storage engine and on-disk format | No hardened kernel or microVM |
| GQL, SPARQL, UQL and Bolt behaviour and conformance | TLS optional on local carriers |
| Placement rules and coordination paths | Resource budgets sized to one machine |

## I/O backend

Docker 25.0.0 and newer block `io_uring_setup`, `io_uring_enter` and `io_uring_register` in the default seccomp profile, because of vulnerabilities that can be exploited to escape containers. TigerBeetle, which has no fallback, fails to start under that profile. The development container therefore detects io\_uring at startup and falls back to compio's polling driver (epoll on Linux, kqueue on a native macOS build). The sans-IO core is unchanged; only throughput differs. For performance testing, a documented seccomp profile re-enables io\_uring for this container only.

## Resources

- **Cores.** The thread-per-core runtime sizes itself from the container's CPU quota. With fewer cores than roles, each core's runtime multiplexes tasks from several roles instead of pinning one role per core.
- **Memory.** The container's memory limit is split into per-role budgets (storage cache, compute working memory, frontend), each overridable.
- **Data volume.** One volume at `/data` with one directory per role and replica (`/data/meta`, `/data/seq`, `/data/storage/g1/r1` and so on), in exactly the layout a cluster node uses for the same role.

## Configuration

The container starts with no configuration: `ultragraph dev` is the default command. Profiles are presets of the same settings the cluster uses, not separate modes.

| Profile | Topology | Carrier | Purpose |
| --- | --- | --- | --- |
| `dev` (default) | 2 storage groups of 1 replica | In-process | Everyday development |
| `dev-ha` | 2 storage groups of 3 replicas | In-process | Failover testing: replicas can be stopped and restarted from the web UI |
| `dev-net` | As `dev` | Loopback TCP | Protocol debugging and traffic capture |
| `frontend`, `compute`, `storage` | One role per process | Network | Cluster deployment |

| Port (proposed) | Purpose |
| --- | --- |
| 7687 | Bolt, for Neo4j drivers and tools |
| One native-protocol port | Binary protocol for native clients |
| One HTTP port | Web UI, admin API, health and metrics |

## Packaging

- **Scratch OCI image** containing only the static musl binary; web UI assets are compiled into the binary. Runs as a non-root user with no shell.
- **Health endpoints:** `/health` for liveness and `/ready` once the metadata group, sequencer and storage groups are all up.
- **Schema on boot:** `.ugs` files mounted at `/schema` are compiled and applied at startup, so a project's schema lives in its repository.
- **Graceful shutdown:** on SIGTERM, frontends stop accepting work, compute drains, and storage groups flush, in that order.

## From development to cluster

- The same schema file, queries and drivers work unchanged.
- Data moves by backup and restore or by export and import. The development container is not grown into a cluster, because replica counts and placement differ.
- An optional Compose file runs one container per role from the same image, for cluster-shaped testing on a single machine.

## Limits

The development container is not for production: replicas share one machine and one disk, the hardened kernel and microVM isolation are absent, and TLS is off on local carriers by default.

## Open questions

- [ ] Should `dev-ha` replace `dev` as the default once the in-process topology is stable?
- [ ] Native-protocol and HTTP port numbers.

## Sources

- [Docker: Seccomp security profiles](https://docs.docker.com/engine/security/seccomp/)
- [moby/moby #46762: block io\_uring syscalls in the default seccomp profile](https://github.com/moby/moby/pull/46762)
- [Dgraph: Basic cluster setup (standalone image)](https://www.dgraph.io/installation/single-host-setup)
- [Dgraph forum: standalone image no longer contains Ratel](https://discuss.dgraph.io/t/getting-started-docs-say-that-standalone-image-contains-ratel-but-it-doesnt/16767)
