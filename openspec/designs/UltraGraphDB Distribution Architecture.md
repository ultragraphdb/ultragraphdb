# UltraGraphDB Distribution Architecture

Oct 31, 2026 · @Marvin Hansen

UltraGraphDB is a distributed graph database with three node types (frontend, compute and storage) replicated with Viewstamped Replication (VSR). Its defining design choice is **schema-driven coordination avoidance**: the typed schema declares every invariant (keys, cardinalities, acyclicity, delete semantics), so the database decides per write whether cross-group coordination is needed at all. Most writes commit in a single replica group; the remainder is ordered Calvin-style. In the current stage every fact is written by its owner in one local commit, snapshot reads wait for closed epochs, and reverse reads reach inbound edges through referrer sets (Shard-per-Core note, P24); derived structures follow through ordered change streams with tiles. The current stage stores data in the Dgraph-derived KV store on direct-attached NVMe. Tile storage, and the S3 mode built on it, come in a later stage. Storage nodes boot a minimal hardened Linux kernel.

## Cluster architecture

Three node types, each its own binary and OCI image under `../../ultragraphdb` (decided 2026-11-01, Shard-per-Core note P22). The split follows StarRocks (frontends plus executors) and matches Apache HugeGraph's independently evolved Server, PD and Store layout, with a separate compute tier because graph algorithms are heavy and should not compete with storage I/O.

&#91;embedded content: Cluster architecture · frontend, compute and two storage modes\]

| Node type | Responsibilities | State | Scales by |
| --- | --- | --- | --- |
| Frontend | Metadata VSR group; binary-protocol endpoint; Bolt adapter; query planning, write classification and scheduling | Small but critical | Non-voting observer frontends for read concurrency |
| Compute | Query execution, graph algorithms, cross-group joins, aggregation, result assembly; tile cache in S3 mode | Stateless apart from cache | Elastic add and remove |
| Storage | VSR replica groups holding the write log and the KV store (tiles in a later stage); applies incoming change streams in the tile stage; runs only narrow kernels near the data (tile scans, frontier expansion with pushed-down filters, intersections) | Durable | Adding replica groups |

Storage nodes run only small, fixed kernels so the hardened storage image stays minimal; everything heavier runs on compute.

## Consistency and coordination avoidance

The schema, not the developer, decides which writes need cross-group coordination. Everything else commits in a single replica group. In the current stage reads stay exact because each fact has one owner that writes it in one local commit, snapshot reads wait for closed epochs and reverse reads find inbound edges through referrer sets (P24); in the tile stage, ordered change streams and watermarks keep derived tiles exact.

### Where the cost actually is

- **Within a replica group, VSR is cheap.** A commit waits for a majority, one round trip, and batching amortizes it across thousands of operations.
- **Crossing groups is expensive.** Transactions touching several predicates, constraints such as unique keys, cardinalities and acyclicity, and consistent snapshots across groups all need agreement between groups. A naive design turns these into two-phase commit on top of consensus.
- **Current stage: bucket placement, one partition per core.** A storage node gives its cores to partitions, each a replica group owning ranges of buckets, and an entity's facts live in its bucket's partition (`UltraGraphDB Shard-per-Core Write Path.md`, decided 2026-11-01). Every fact stays with its writer: the subject's partition writes an edge's forward and reverse entries, its predicate lists and its indexes in one local commit, and the object's home keeps only a referrer set naming the bucket groups that may hold edges to it, which a reverse read consults before it fans out (Shard-per-Core note, P24, decided 2026-11-02). A hub's inbound entries are therefore spread over its referrers' partitions. Predicate sharding, Dgraph's layout, would bound a hot predicate to one core.
- **Later stage: 2D tiling.** Tiles fix hubs but separate forward and reverse entries, so in the tile stage reverse tiles will be maintained through change streams. The change-stream mechanisms below belong to the tile stage; the rest are designed for both stages.

### Principle: coordinate only where an invariant could break

*Invariant confluence* (Bailis et al., VLDB 2015) is the formal criterion: a set of operations can run without coordination exactly when merging concurrent executions always preserves the invariants. Ten of TPC-C's twelve invariants qualified, and the prototype reached over 12.7 million New-Order transactions per second on 200 servers, 25 times prior results. The CALM theorem gives the intuition: monotonic operations, which only add information, never need coordination.

The idea stayed in research prototypes because developers had to state their invariants and design segmentations by hand, which the follow-up work itself calls onerous. UltraGraphDB's typed schema already declares them, so the analysis can be automatic.

&#91;embedded content: Write path · classification, change streams and watermarks\]

### Write classification

The classification is computed at schema compile time as a conflict graph over write operations and constraints (in the style of Hamsaz), stored in the catalog, and applied by the planner per write.

| Write | Invariant at stake | Path |
| --- | --- | --- |
| Insert edge of an unconstrained relation | Referential integrity only | **Coordination-free:** commit in the subject's partition with its reverse entry (KV stage, P24); reverse tiles follow by change stream (tile stage) |
| Create entity | ID uniqueness | **Coordination-free:** IDs from per-group ranges |
| Update text, vector or statistics indexes | None (derived data) | **Coordination-free:** text indexes and statistics are written in the subject's partition's local commit (KV stage, P24); the vector indexer alone runs asynchronously, outside the transaction |
| Set a `@key` or `@unique` property | Uniqueness of a chosen value | **Segmented:** unique index partitioned by key hash; the owning group checks |
| Insert edge into a cardinality-constrained role | Cardinality | **Segmented:** constrained-role edges stored in the constrained endpoint's group |
| Change a bounded numeric value | Threshold | **Segmented via escrow:** each group holds a share of the bound |
| Insert edge into an `@acyclic` relation | Acyclicity | **Segmented**: rank-based local checks, with each hierarchy colocated in one group; edges across hierarchies are **coordinated** |
| Delete entity | Referential integrity | **KV stage:** stage 5.5 deletes by pattern only, refused with `CrossPartition` when a delete reaches another partition; from stage 6 an entity delete writes a tombstone at the home, and stage 8 sends sequenced shares fencing it at the members of its referrer set (**coordinated**, P24 rules 7–8). **Tile stage:** declared semantics, cascade via change streams; restrict is **coordinated** |

### Mechanisms

- **Ordered change streams, not CRDTs.** Forward writes are already totally ordered by VSR in their source group. Derived updates carry (source group, operation number), and each target group applies every source's stream in order. Deletes are therefore exact, with no tombstone sets or add-wins ambiguity, and replays are harmless.
- **Watermarks.** Each group publishes how far it has applied each incoming stream. A snapshot query waits until the watermarks of the groups it reads cover its snapshot.
- **Constraint locality.** Placement rules put each constraint's state in one group: unique indexes by key hash, constrained roles with the constrained endpoint. Locality-aware placement of densely connected regions keeps most writes single-group.
- **Escrow treaties** (from the Homeostasis protocol): bounded constraints are split into per-group shares, renegotiated only when a share runs out.
- **Declared delete semantics** (inspired by IPA): cascade, restrict or orphan-to-tombstone, so concurrent "delete node" and "add edge to node" have a defined outcome.
- **Background verifier.** Continuously checks reverse entries against forward edges and repairs differences.
- **The coordinated remainder** uses deterministic transaction ordering (Calvin-style sequencing, which pairs naturally with VSR's deterministic state machines): multi-group transactions are sequenced into a replicated log first and then executed deterministically by every participating group, with no two-phase commit and no distributed-deadlock aborts.

### Query consistency levels

**The guarantee (decided 2026-11-01).** Transactions are strictly serializable within a replica group. Across groups they are serializable at a closed epoch, the default, with session tokens for read-your-writes; a per-request option closes the current epoch first, for a strict cross-group read.

Queries are classified too, following *Keep CALM and CRDT On* (VLDB 2023), which showed that coordination-free updates still leave reads unsafe unless queries are analyzed.

| Level | Applies to | Guarantee |
| --- | --- | --- |
| Snapshot (default) | All queries | Waits for watermarks, which are closed epochs; exact, standards-conformant results |
| Monotonic-fast (opt-in) | Monotonic queries only: existence, reachability, pattern matches | Answers immediately; every result was true at a recent closed epoch and stays true for append-only or immutable data; recent facts may be missing |

Non-monotonic queries (`NOT EXISTS`, counts and aggregates, absence-sensitive optional matches) always run at snapshot level. Monotonic-fast suits agent retrieval, where missing a little recent context is acceptable. Over relations declared append-only in the schema it can never return a false fact; over mutable data the response is flagged, since a fact deleted after the replica's last closed epoch may still appear.

Both levels are exposed in GQL and SPARQL as well as the native protocol. Since neither standard defines read consistency levels, the setting is an implementation-defined extension: a GQL session parameter, flagged by the GQL Flagger, and a SPARQL protocol option, plus a per-request flag in the binary protocol. A request for monotonic-fast on a non-monotonic query falls back to snapshot level and says so in the response.

### Prior art and lessons

| System or work | What it showed | Lesson applied |
| --- | --- | --- |
| RedBlue / Gemini (OSDI 2012) | Synchronized red and asynchronous blue operations | Labelling was manual; derive it from the schema |
| Coordination avoidance (VLDB 2015) | 25-fold TPC-C throughput by coordinating only where needed | Classify every write |
| Lucy (VLDB 2019) | Segmented confluence; up to 10x linearizable throughput at low contention | Segment by constraint ownership |
| Hamsaz (POPL 2019) | Conflict graph; minimal synchronization via maximal cliques | Compile-time analysis |
| IPA (VLDB 2019) | Modify operations so concurrency cannot break invariants | Declared delete semantics |
| Homeostasis (SIGMOD 2015) | Treaties let sites run independently within bounds | Escrow for numeric constraints |
| Facebook TAO and RAMP-TAO (VLDB 2021) | 1 in 1,500 batched reads saw partial transactions, some for up to 13 seconds; a read-atomic layer fixed it in one round trip for over 99.93% of reads at 0.42% memory overhead | Build read atomicity in: watermarks |
| Cassandra materialized views | Experimental since 3.0.16 and 3.11.2: no resynchronization, unbounded lag, delete and expiry divergence | Ordered streams, verifier, formal model |

CRDTs remain the right tool for one future case: replicas that must keep working while disconnected and merge later, such as edge sites on intermittent networks.

## Metadata group

Any cluster with more than one replica group needs agreement on placement, snapshots and versions. Dgraph uses a separate node type for this (Dgraph Zero); StarRocks folds it into its frontends, replicating metadata across a leader, voting followers and non-voting observers. UltraGraphDB follows StarRocks: the metadata state machine runs as a VSR group on the frontends.

**Responsibilities**

- **Membership:** which nodes exist and which replica group each belongs to.
- **Placement:** bucket ranges per partition, tenant runs and bucket groups resolved at an epoch now (P1, P2, P24), GSL tile assignment later, constraint-locality rules, and changes during rebalancing.
- **Sequencing:** the Calvin-style log that orders multi-group transactions.
- **Snapshots:** consistent cross-group snapshot points (see below).
- **ID ranges:** assigning bucket ranges to partitions and leasing sequence blocks per (type, partition). A system ID is 128 bits: from the most significant bit, a 16-bit type prefix, a 64-bit sequence unique per type, a 24-bit bucket (8 bits hashed from the tenant, 16 from the key) and 24 reserved bits (decided 2026-11-01, Shard-per-Core note P7, P15, P16; field order type, sequence, bucket, reserved by P24 rule 1, 2026-11-02).
- **Versions:** schema catalog, the write and query classification derived from it, the RDF/LPG mapping contract, and the finalized feature level.

**Its home from stage 5.5 (decided 2026-11-01).** The metadata group's state machine is built in stage 5.5, log-backed like the commit step and with one replica, holding catalog versions, ID ranges and the placement map, with catalog contents opaque to it. UGS (stage 6) is built on it; stage 8 adds its replication.

**Keeping it small**

- **Sequencer epochs** for snapshots instead of per-transaction timestamps, so no commit waits on the metadata group.
- **Deterministic placement** derived from the bucket in every ID, and from GSL coordinates with tiles, so lookups need only the short placement map of bucket ranges, and consensus is only needed when the mapping changes.
- **Sequences leased in blocks per (type, partition)** instead of an ID per round trip; a bucket moves between partitions without changing an ID.

With these, only membership, the placement map and the schema, classification and feature versions need consensus.

### Snapshot timestamps: sequencer epochs

Cross-group snapshots are defined by **sequencer epochs**. This was chosen over a timestamp service, which puts a round trip and a single leader on every commit path, and over hybrid logical clocks, which depend on bounded clock skew, cause read restarts and put clock values on every change stream.

**How it works**

1. The sequencer, its own VSR group on the frontend nodes, closes an epoch every few milliseconds and broadcasts the boundary. The same log orders Calvin-style multi-group transactions. Each closed epoch records the sequencer's wall-clock time at its close, never below the previous epoch's, so `as of time t`, `now()` and transaction time resolve to an epoch; ordering never reads the clock.
2. Every commit is stamped with an epoch: single-group commits with one above the latest close delivered to the partition (P24 rule 4), multi-group transactions with the epoch the sequencer placed them in. In the tile stage, change-stream entries carry their epoch.
3. A partition **closes** epoch *e* once it has received *e*'s close and applied every round stamped at or below *e*, and records the operation number at that boundary; in the tile stage it also waits for every incoming stream entry stamped up to *e*.
4. A cross-group snapshot is an epoch: the query reads epoch *e* in every group it touches once each has closed it. Watermarks are simply closed-epoch markers.

**Design choices that minimize the costs**

| Cost | Mitigation |
| --- | --- |
| Snapshots lag real time by up to one epoch | Short epochs in the low milliseconds, cheap because closing an epoch is a counter broadcast; queries confined to one group read its latest committed state with no epoch wait; monotonic-fast reads never wait |
| Read-your-writes must wait for the write's epoch to close | Commits return a session token (group, operation number, epoch). Single-group reads use the operation number, so they never wait; multi-group reads can ask the sequencer to close the current epoch immediately, bounding the wait to about one round trip |
| A causally later write in another group could receive an earlier epoch | **Epoch propagation rule:** requests (and, in the tile stage, change streams) carry the highest epoch the session or sender has seen. A partition stamps exactly one above the latest close delivered to it and never advances on its own, so it waits for the close below a carried epoch before it commits (P24). Epochs behave like logical clocks, so causally later writes never appear in earlier snapshots |
| Sequencer unavailability stalls new snapshots | The sequencer is a VSR group and survives minority failures. During a view change, single-group writes and reads and monotonic-fast reads continue; multi-group snapshot reads may fall back to the latest closed epoch, with the staleness reported in the response |
| One sequencer may saturate | Only epoch boundaries and multi-group transactions pass through it, all batched. If it saturates, sequencing is partitioned across several sequencers whose logs merge deterministically per epoch, as in Calvin's own design |
| Epoch length is a trade-off | Adaptive within fixed bounds: shorter while sessions wait on read-your-writes, longer under heavy batch load |

**Side benefits.** Epochs give natural point-in-time queries ("as of epoch *e*"), reasoning runs that can be reproduced by pinning an epoch, and a clock-free, deterministic protocol the simulator can test exhaustively. Hybrid logical clocks would be reconsidered only if multi-region active-active deployment becomes a requirement, since a single sequencer would then put an inter-region round trip into every epoch.

## Replication and runtime model

Replication is VSR throughout, and every core runs its own event loop with no shared state on the hot path.

- **VSR replica groups** for storage, plus one VSR group for metadata on the frontends. Stage 8 builds them first, on the single container's `dev-ha` profile, and stage 10 at scale. `UltraGraphDB Distribution Protocol.md` is the protocol: storage groups acknowledge from memory on a quorum with a bounded exposure, and the metadata group and the sequencer acknowledge from disk.
- **Thread-per-core with compio.** Completion-based I/O (io\_uring on Linux), with each core owning its submission and completion queues, NVMe queue pair and NIC queue.
- **Sans-IO core.** VSR, the storage engine and the query engines take I/O through traits. Implementations: the deterministic simulator, Linux io\_uring, and any future backend without touching core logic. The same design gives native sync and async clients.
- **Batching amortizes consensus.** TigerBeetle, the closest comparable system, processes batches of up to 8,190 transfers per request and is designed for about one million transactions per second on a single core.
- **Scaling VSR itself.** Decoupling and partitioning protocol roles, analyzed for safety through monotonicity, raised Paxos throughput 3x (on 29 machines against 8) and two-phase commit 5x in recent work (SIGMOD 2024), raising throughput with negligible added latency, and reducing none. The same treatment applies to VSR: batching, ordering and execution as independently scaled roles, as in Compartmentalized Paxos.
- **Internal traffic uses the same binary protocol** as clients: Iggy-style byte-level frames, Arrow-compatible column buffers, version negotiation one version back.

## Storage modes

The current stage runs in direct-attached mode on the KV store. S3 mode arrives with tile storage, because immutable tiles are what make object storage and compute-side caching safe. With S3 as the backend the storage tier does not disappear; it shrinks into a log tier, because commits must be durable faster than S3 writes allow.

|  | Direct-attached mode | S3 mode |
| --- | --- | --- |
| Frontend | Same | Same |
| Compute | Executes; reads tiles from storage nodes | Executes; local NVMe tile cache, reads from S3 on a miss |
| Storage | Full data: log, deltas and tiles on NVMe | Log tier only: log and recent deltas; tiles in S3 |
| Best for | Lowest latency, on-premises, air-gapped, high-assurance | Elastic cloud, large data, low storage cost |

S3 mode also works on premises with S3-compatible stores such as MinIO or Ceph.

### Write path in S3 mode

- Writes are acknowledged once replicated in the log tier's VSR group on local NVMe. Standard S3 writes take tens of milliseconds; S3 Express One Zone is faster but still slower and costlier per request than an NVMe flush.
- Log nodes, or a dedicated compactor role, merge deltas into new immutable tile versions and upload them; once a tile is in S3, the log can be trimmed.
- S3 conditional writes make a log directly on S3 possible (as in SlateDB), which suits batch-heavy workloads but not interactive graph writes.

### Read path and caching

- **Immutable tiles make caching safe.** Tile versions are stamped with an epoch and never change, so a cache only needs to know which version a snapshot references.
- **The cache lives on compute nodes** as local NVMe. A separate shared cache tier is worth it only if per-node caches duplicate a lot of data.
- **Cold traversals pay S3 latency at every hop.** Prefetch the next hop's tiles in parallel once the frontier's GSL coordinates are known; size tiles in megabytes; pin hot tiles and warm new compute nodes from peers instead of S3.

## Tile storage and query execution (later stage)

Tile storage is postponed; the current stage uses the Dgraph-derived KV store with bucket placement, which is adequate for now. When tiles arrive, graph topology becomes a hierarchically tiled sparse matrix: the tiling decides where data lives, how supernodes are split and how much each hop costs, and it moves reverse adjacency onto change streams. The multi-hop principles below apply to both stages.

### Tile layout

- **Dense, type-clustered IDs.** The entity type sits in the high bits, assigned in preorder of the type hierarchy, so a subtype is a contiguous range and a supertype one larger range. Tiles use hypersparse formats (DCSR and DCSC). The group ID also sits in the ID, so IDs are generated without coordination.
- **One matrix per relation or role, stored both ways:** CSR for forward traversal, CSC for reverse. N-ary relations are one incidence matrix per role. Reverse tiles are maintained through the change streams.
- **Adjacency ordered by neighbor type and key edge attributes,** so type filters and common edge filters become range reads.
- **Embeddings** are per-type columnar attributes stored in BF16, with a quantized index for search and BF16 rescoring of the top candidates.

### GSL addressing

| Level | Meaning | Role |
| --- | --- | --- |
| G (group) | Availability group | Replication and failover, one VSR group per G |
| S (shard) | Position in the 2D tile grid | A row range by column range assigned to the group |
| L (local) | Tile on a node | Unit of paging, caching, versioning and compaction |

With 2D partitioning each traversal step communicates along one process row and one process column, about the square root of p peers instead of all p. A supernode's adjacency row is split across column tiles and machines automatically.

### Multi-hop execution

The cost of hop k is roughly the number of edges touched from the current frontier, growing by an effective branching factor (average degree times the fraction of edges passing the filter). Depth beyond two hops stays fast when:

- **filters are pushed into the expansion as range reads,** so cost scales with surviving edges;
- **the frontier is deduplicated,** bounding each hop by the number of vertices;
- **queries ask for reachability or aggregates** rather than enumerating every path;
- **the planner chooses direction:** start from the more selective end, search from both ends when both are known, and switch to pull-direction traversal for large frontiers.

### Lessons from StarRocks

| StarRocks technique | Graph equivalent |
| --- | --- |
| Vectorized columnar batches | Frontier-at-a-time expansion over sorted ID lists |
| Colocate joins | GSL placement aligned with relation partitioning, so most hops stay local |
| Global runtime filters | Filters built from one side of a pattern pushed into the other side's expansion |
| Low-cardinality global dictionary | Dense, type-prefixed IDs and the IRI dictionary |
| Cascades-style cost-based optimizer | Cost-based planner with distribution strategies as physical choices |

## Node runtime and hardening

Deployment targets (decided 2026-11-01): Kubernetes and Docker, bare metal later. CI builds, signs and publishes the OCI images with Bazel's `rules_img`, the server's glibc base built from Wolfi packages by `rules_apko` (D7). The microVM runtime and minimal kernel described below are tracked options, not current targets.

Each role runs its own binary, linked against glibc with mimalloc, on the Wolfi glibc base of `../../bazel/oci_images/glibc_base` (D7, decided 2026-11-01): musl with mimalloc measured about 18% slower per core. The tracked microVM option builds a static musl variant for its kernel.

| Node type | Runtime |
| --- | --- |
| Storage / log | The storage binary in an OCI container on the glibc base, with a seccomp profile allowing io_uring; a static musl variant on a minimal custom Linux kernel in a dedicated microVM (Firecracker or Cloud Hypervisor) stays a tracked option |
| Frontend | The frontend binary, holding the metadata group and the sequencer, in an OCI container on the glibc base; the hardened microVM image is an option for high-assurance deployments, since frontends hold the metadata group |
| Compute | The compute binary in an OCI container on the glibc base, elastic under an orchestrator |

### Minimal kernel for storage nodes

The approach follows Lupine Linux (EuroSys 2020): a standard Linux kernel specialized through its configuration achieves unikernel-like performance while keeping Linux's drivers, tooling and security scrutiny.

- **Enabled:** 64-bit, SMP, KVM guest, kvm-clock and PTP, virtio-blk and virtio-net, NVMe and AWS ENA, io\_uring, TCP/IP, XFS or raw block access, virtio-rng, seccomp.
- **Disabled:** loadable modules entirely, USB, sound, graphics, unused file systems, kexec, debugfs, /dev/mem, unprivileged BPF, user namespaces.
- **No userland:** the storage binary is the only process, from a read-only initramfs; no shell, package manager or ssh.
- **io\_uring restricted** to the exact operations compio uses via IORING\_REGISTER\_RESTRICTIONS (io\_uring operations bypass seccomp); the io\_uring\_disabled and io\_uring\_group sysctls (Linux 6.6) limit who may create rings.
- **Hardening:** KASLR, lockdown mode, stack protection, memory initialization on allocation, hardened user copies, Clang control-flow integrity.
- **Signed, reproducible image,** with measured boot or confidential-computing attestation (AMD SEV-SNP, Intel TDX) as options; a debug variant from the same source enables ftrace, perf and eBPF.

### libc and system calls

- **glibc, dynamically linked,** for its vectorised string routines; the loader stays current through the base's lock, updated by `bazel run //bazel/oci_images/glibc_base:lock`. Client tools ship static musl binaries on scratch.
- **mimalloc as the global allocator,** in secure mode subject to benchmarks.
- **Pure-Rust, no\_std DNS resolver,** fuzzed continuously; the libc resolver is never called.
- **rustix with the linux\_raw backend** for direct system calls, including vDSO time reads.
- **io\_uring through direct system calls:** the io-uring crate's direct-syscall feature now; port compio's backend to rustix's io\_uring module upstream later. Container runtimes refuse io\_uring under their default seccomp profiles, so the images ship a profile allowing `io_uring_setup`, `io_uring_enter` and `io_uring_register`.
- **Symbol allowlist in CI:** the release binary may link only an approved set of libc symbols; anything else fails the build.

### Unikernel options (tracked, not default)

- **Hermit:** Rust research unikernel; storage only through virtio-fs, no known production users.
- **Nanos:** Apache-2.0 source with commercial support; 27 io\_uring operations, unchanged since release 0.1.53 (December 2024); a very small kernel team. Worth evaluating for customers who require a non-Linux image.

## Security by tier

| Tier | Exposure | Controls |
| --- | --- | --- |
| Storage / log | Durability boundary; holds all data | Hardened minimal-kernel image, io\_uring allowlist, no userland; only these nodes (or the compactor) hold S3 write credentials |
| Compute | Data-bearing in S3 mode through its cache; runs the largest code surface | Encrypted cache; read-only S3 credentials scoped to tile objects, so a compromised node can read but not corrupt or delete |
| Frontend | Client-facing parsers and protocols; holds metadata | TLS and authentication in the protocol handshake; every inbound frame bounds-checked before zero-copy views are handed out |

Change streams between groups are authenticated like client traffic, so a compromised node cannot inject derived updates into another group. The hypervisor is the isolation boundary between nodes; most Linux io\_uring CVEs are local privilege escalations, which matter little in a single-purpose VM where the database process already owns the data.

## Rolling upgrades

Storage upgrades first, compute second, frontends last, and new formats activate only after an explicit cluster-wide finalize step.

### What StarRocks does

- Backends and compute nodes are backward compatible with frontends, so they are upgraded first; frontends follow, followers before the leader. Downgrade is the exact reverse.
- Graceful exit (v3.3 onward) drains running query fragments before a node stops; since v3.4 frontends recognize an exiting backend instead of marking it dead.
- Some changes are one-way: the BDB JE upgrade in v3.0 cannot be rolled back, and v4.1 limits downgrades to v4.0.6 or later.

### Upgrade order

| Step | Tier | How |
| --- | --- | --- |
| 1 | Storage / log nodes | One replica at a time per VSR group, never more than the group can lose; wait for log catch-up. Replace the signed microVM image and reattach the data volume; rollback boots the previous image. |
| 2 | Compute nodes | Drain and replace; in S3 mode, warm the cache from peers before taking traffic. |
| 3 | Frontends | Followers first, leader last. Gateways roll with compute. |

Storage goes first because it holds durable formats; frontends go last so the planner only emits new plan features, and only publishes a new write classification, once every node understands them.

### Rules

- **Agree on the state-machine version through the log.** Each binary contains the current and previous state-machine logic, and the switch is an operation committed through VSR, so every replica changes logic at the same point in history. TigerBeetle bundles earlier versions in its releases for the same reason.
- **Binaries first, features later.** Roll out binaries while every node keeps writing old formats; a finalize step committed through the metadata group then enables new tile formats, plan features, protocol fields and change-stream formats. Before finalize, downgrade is a binary rollback; after, it is one-way and stated as such.
- **Readers handle the current and previous formats;** old-format tiles age out through compaction.
- **Change streams carry a format version,** so a target group running the older binary rejects nothing it can still apply.
- **Internal protocol negotiation supports one version back,** and the compute cache is keyed by tile version and format.

## Verification strategy

Coordination avoidance moves correctness from runtime coordination into analysis and protocol design, so verification carries more weight than usual.

- **Formal model first.** A Lean model of epochs, registration, referrer sets, reclamation and deletes (P24 rules 4, 5, 6, 9 and 11, stage 5.5's `../../lean/PlacementModel`, including the epoch propagation rule), of the change-stream and watermark protocol in the tile stage, and of the state-machine version switch, proved before implementation, in the repository's `lean/` project beside stage 5's round model (decided 2026-11-01). Cassandra's materialized views failed partly because their design was never extensively modeled or simulated.
- **Classification soundness.** Property tests check, for every constraint the schema can express, that any write classified coordination-free or segmented preserves the invariant under all interleavings the simulator can generate.
- **Deterministic simulation.** Fault injection, crash recovery, network partitions and mixed-version clusters, with results compared against a single-version, single-group reference run.
- **Background verifier in production.** Continuous comparison of reverse entries against forward edges and of derived indexes against base data, with repair and alerting.
- **Conformance suites on every build:** W3C SPARQL, the openCypher TCK and the GQL scenario suite, run against clusters, not just single nodes, at snapshot consistency.

## Decisions

| Question | Decision |
| --- | --- |
| Storage engine | Dgraph-derived KV store now; tile storage in a later stage |
| io\_uring without libc | io-uring crate's direct-syscall feature now; porting compio to rustix's io\_uring module stays an option |
| Frontend runtime | Scratch OCI container by default |
| Monotonic-fast reads | Exposed in GQL, SPARQL and the native protocol |
| Coordinated remainder | Calvin-style deterministic ordering |
| Acyclicity | Rank-based local checks, with hierarchies colocated in one group |
| Snapshot timestamps | Sequencer epochs, with the mitigations under Metadata group |

## Open questions

- [ ] Epoch length bounds for the adaptive sequencer, measured against snapshot freshness and sequencing overhead.
- [ ] Sequencer placement: its own VSR group on the frontends (current plan), or merged into the metadata group for small clusters?
- [ ] Tile size for S3 mode, once tile storage starts.

## Sources

- [Coordination Avoidance in Database Systems (Bailis et al., VLDB 2015)](https://arxiv.org/pdf/1402.2237)
- [Interactive Checks for Coordination Avoidance (Whittaker and Hellerstein, VLDB 2019)](https://mwhittaker.github.io/publications/iconfluence_VLDB19.pdf)
- [Hamsaz: Replication Coordination Analysis and Synthesis (POPL 2019)](https://mohsenlesani.github.io/companion/popl19/POPL19.pdf)
- [IPA: Invariant-Preserving Applications (VLDB 2019)](https://www.dpss.inesc-id.pt/~rodrigo/vldb-2019.pdf)
- [The Homeostasis Protocol (SIGMOD 2015)](https://infoscience.epfl.ch/record/219208/files/final_camera_ready.pdf?version=1)
- [Keep CALM and CRDT On (VLDB 2023)](https://www.vldb.org/pvldb/vol16/p856-power.pdf)
- [Optimizing Distributed Protocols with Query Rewrites (SIGMOD 2024)](https://arxiv.org/pdf/2404.01593)
- [Making Geo-Replicated Systems Fast as Possible, Consistent when Necessary (OSDI 2012)](https://web.eecs.umich.edu/~manosk/assets/papers/gemini.pdf)
- [RAMP-TAO (VLDB 2021)](https://www.vldb.org/pvldb/vol14/p3014-cheng.pdf)
- [Instaclustr: Apache Cassandra Materialized Views](https://www.instaclustr.com/blog/apache-cassandra-materialized-view-instaclustr-support/)
- [StarRocks: Architecture](https://docs.starrocks.io/docs/introduction/Architecture)
- [StarRocks: Upgrade StarRocks](https://docs.starrocks.io/docs/deployment/manage_deployment/upgrade/)
- [StarRocks: Graceful Exit](https://docs.starrocks.io/docs/administration/management/graceful_exit)
- [StarRocks: Downgrade StarRocks](https://docs.starrocks.io/docs/deployment/downgrade)
- [Lupine: A Linux in Unikernel Clothing (EuroSys 2020)](https://people.cs.vt.edu/djwillia/papers/eurosys20-lupine.pdf)
- [TigerBeetle: Deploying on Linux](https://docs.tigerbeetle.com/operating/linux)
- [TigerBeetle issue #1940: throughput discussion](https://github.com/tigerbeetle/tigerbeetle/issues/1940)
- [Nanos releases](https://github.com/nanovms/nanos/releases)
- [Linux: io\_uring\_disabled sysctl commit](https://lab.nexedi.cn/kirr/linux/-/commit/76d3ccecfa186af3120e206d62f03db1a94a535f)
- [io-uring crate: direct-syscall feature](https://docs.rs/crate/io-uring/latest/source/src/sys/mod.rs)
- [compio-driver 0.12.0 Cargo.toml](https://docs.rs/crate/compio-driver/0.12.0/source/Cargo.toml)
