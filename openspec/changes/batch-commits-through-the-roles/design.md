## Context

The commit step groups commits into rounds, one log append and one sync each (`commit-rounds`), and in process it does:
`wal_bench` offers commits faster than a sync takes, so each round closes over many. Through the binaries every round holds one
commit (stage 5.5, 12.5b part 2, experiment X1). Three facts make it so, each read in the code:

1. `serve_connection` (`ultragraphdb_common_role/src/utils/serve.rs:184`) reads a connection's frames into a pool of **one**
   buffer of `MAX_FRAME_SIZE`, so the connection reads its next frame only once the last `ReceivedFrame` is dropped, and it
   awaits `Session::answer` for each request before reading on.
2. `StorageSession::answer` (`ultragraphdb_common_storage/src/types/storage_session.rs`) sends the `ReceivedFrame` to its
   partition core in a `CoreMessage::Request` and awaits a `oneshot` the core answers when it **takes** the request. For a
   Commit or a one-frame write the core answers `Answer::Open` at once and pushes the reply after the sync, so the session does
   not wait for the sync, only for the take.
3. `PartitionCore::run` takes its inbox between turns, and a turn awaits every sync it needs (`PartitionHost::turn`:
   `while self.pipeline.needs_sync() { self.pipeline.sync(now).await }`). While a sync is in flight nothing is taken.

So a role connection holds one request at most in the core's inbox, and the frontend sends every client's writes for a partition
over one role connection (`PROTOCOL.md` §11.12, *Frames on their way*): when a sync ends, one commit waits, and it syncs alone.
X1 found exactly one log sync per commit in every run and 17.2 ms of a 19.9 ms commit spent before the core at 16 connections.

`Session`'s own contract already names the way out: "a request waiting on something beyond the session's own turn answers
`Answer::Open`, its frame copied and counted in `Session::parked` and its answer pushed". The partition core does that for a
request that waits on a home; this change does it for every request.

The second half of the change answers the owner's direction to test on a real Kubernetes deployment, locally, on Apple's
`container` runtime (1.7.1 installed; `container cluster` is experimental, kind's `kindest/node` image, nodes as VMs, Apple
silicon only). Kubernetes is a decided deployment target (backlog item 1), the role-binaries spec already names a Kubernetes
Secret for the role secret, and the config service's *Frontend* rule already lets a Service's name stand for the frontend's host.
No manifest exists yet.

## Goals / Non-Goals

**Goals:**
- Commits a partition receives while its log syncs form the next round, however many arrive on one role connection, within
  `storage.held_bytes`.
- No change to any frame, command or error on the wire, nor to the commit step, the log or recovery.
- The roles deployed on Kubernetes from manifests in the repository, run on a local `container` cluster from the images Bazel
  builds, with the bench running inside the cluster.
- The batching measured through the binaries against the baseline the Shard-per-Core note records, on a `container` Linux
  machine and on the cluster.

**Non-Goals:**
- Taking the inbox while a sync is in flight (overlapping the sync with the next round's decisions): batching does not need
  it, and it means splitting `Pipeline::sync` so the log's sync no longer borrows the pipeline. Measured first, decided after.
- The other open items of backlog item 18 (the owner's epoch-close sync, referrer-set reads, a Scan turn's cost), each its own
  question.
- A production Kubernetes deployment: TLS between roles (stage 8), more than one frontend, an operator, amd64
  nodes, autoscaling. The manifests deploy one cluster of the stage-5.5 roles for test.
- Power-loss durability on the cluster's disks; the node VMs' disks are what they are.

## Decisions

### D1. The session copies every request and answers `Open`; the core pushes every answer

`StorageSession::answer` copies the frame into a `ParkedFrame` counted against the connection's `ParkedBytes`, drops the
`ReceivedFrame` so the connection's one buffer is free, sends the copy to the core and returns `Answer::Open`. The connection
reads its next frame at once, until its held bytes reach `storage.held_bytes`. The core runs each request at its next turn as
today, and every answer, a reply, a refusal or an end with no frame, leaves through the connection's push channel.

Alternatives:
- *A pool of several buffers per connection.* Each buffer is `MAX_FRAME_SIZE`, 1 MiB, whatever the frame's size, and a commit
  frame is a few KiB, so N requests in flight would pin N MiB for N KiB of frames. Rejected; a copy of the frame's bytes costs
  what the frame weighs.
- *Several role connections per partition at the frontend.* A batch would be capped at the number of connections and each
  connection still holds one request. Rejected.
- *Hand the `ReceivedFrame` itself to the core without waiting.* The connection could not read on: its one buffer is in the
  frame. Rejected.

### D2. One ordered push channel carries every answer, with a stream's end as a push of its own

Today an answer travels by the `oneshot` and a later frame by the push channel, two channels whose order is patched by
`StorageSession::ending`, which turns a refusal crossing a pushed `LAST` frame into `Open`. With every answer pushed, the core's
order is the order on the wire. `Session::next_push` yields `(stream, Push)` where `Push` is a frame, `End`, a stream ending
with no frame (an Abort's `Answer::Done` today), or `Refuse`, a refusal. `serve` sends a frame on an open stream and drops one for a
stream no longer open; ends the stream on `End` as it does on `Answer::Done`; and sends a refusal through the machine's `refuse`, as
it does `Answer::Refuse`, so a refusal reaches the peer on the connection's own queue for a stream the connection never opened, a
Write on a stream no Bind opened among them. A pushed Error frame would be dropped there, which group 3 found as a hang of
`ultragraphdb_storage_tests`. A refusal crossing the peer's Cancel reaches it as a late frame, which the frontend drops. The `done`
channel and `ending` go.

Alternative: keep each request's `oneshot` in a queue in the session and turn its answer into a push. Two sources of order
again; rejected.

### D3. The bound is the connection's held bytes, the turn's intake unchanged

A copy is counted from the moment it is made until the core drops it: a request run at once at the core's take, a parked one
when it is answered. The core moves a parked request's copy into its `Parked` entry instead of copying it a second time. So
`held_bytes` bounds a connection's requests in the inbox and parked together, and the connection holds at most `held_bytes` and
one frame (`PROTOCOL.md` §11.12). A turn still takes at most `MESSAGES_PER_TURN_MAX` (1,024) messages; what is left waits for the
next turn, and the commit step's round bounds (maximum frame, transaction count) are unchanged. The push channel's length is
bounded by the requests held plus one push per open stream, which the session's documentation states (Ultra rule 2).

Stopping still cannot deadlock (§11.12): what releases a copy waiting for the core is the core's own turn, which needs nothing
from any connection.

### D4. `PROTOCOL.md` §11.12 states the reading, not a new rule on the wire

*Parked requests* gains that a partition copies every request and reads on without waiting for its core, and that those copies
count against `held_bytes` with the parked ones. A peer sees the same frames in the same per-stream order, sooner.

### D5. The Kubernetes shape follows the roles' state and their instance names

- The config service, the frontend and **each storage instance** are a StatefulSet of one replica with a volume for the data
  directory; compute is a Deployment. A storage instance's name is `storage_<label>` and comes from the pod's environment
  (`ULTRAGRAPHDB_CONFIG_INSTANCE`); a StatefulSet's ordinal gives `storage-0`, which no instance may be named, so each instance
  is its own StatefulSet. The frontend is one replica because the metadata group and the sequencer it hosts hold one.
- Compute's pod sets `hostname: compute` and `subdomain: compute`, so its role address is
  `compute.compute.<namespace>.svc.cluster.local` as a StatefulSet pod's is `<pod>.<service>.<namespace>.svc.cluster.local`; the
  kubelet writes that name into the pod's `/etc/hosts`, so the role binds it before any DNS record exists. One replica, replaced
  by `Recreate`, so no two pods hold the name at once.
- The config service runs as its image's user, 65534, as `config-service` states; the roles as 65532.
- Every Secret is mounted `optional`, so a missing one leaves its file absent and the role exits with code 78 naming it, rather
  than the pod waiting unstarted; the chart draws the Secrets' values at install, so `deploy/helm/` holds none.
- A headless Service per workload gives the stable names the role addresses use; `publishNotReadyAddresses: true` lets a role
  resolve and bind its own name before it is ready (role addresses accept host names, resolved as the connection opens, and 12.6
  ran the roles bound to their containers' names).
- A Helm chart, `deploy/helm/ultragraphdb` (the owner's choice, 2026-11-09, replacing the plain manifests and `kustomization.yaml`
  first written: one definition, installable on any cluster). Its objects take fixed names, `config`, `frontend`, `storage`,
  `compute`, which the role addresses name, so one release per namespace. Its values: the images and pull policy, the cluster
  domain, the volumes' class and size, the seccomp profile, `secrets.create`, the bench Job.
- The `kube` profile is the chart's, `profile/`, its addresses the Services' names in the namespace `ultragraphdb` under
  `cluster.local`, which the chart replaces by the release's; the config battery checks it beside `dev` and `dev-net`. It lives in
  a ConfigMap; the config service's pod restarts when it changes, by a checksum annotation.
- The chart draws the keys and secrets (`randAlphaNum 64`) at the first install and reads them back on an upgrade (`lookup`); each
  carries `helm.sh/resource-policy: keep`, as the volumes outlive an uninstall and the frontend's metadata holds the bootstrap
  administrator's verifier. `helm template` reads no cluster and draws anew on each render, so the script creates the Secrets only
  where absent. No operator. Pods run as user
  65532, read-only root, all capabilities dropped, no privilege escalation; probes on `/health` (startup, liveness) and `/ready`
  (readiness).
- Volumes use the `standard` StorageClass of kind's local-path provisioner. A `container` cluster does not install it, but its node
  image holds the manifest, `/kind/manifests/default-storage.yaml`, and the provisioner's images; the script applies it from the
  node after creating the cluster (`notes/group-0.md`, 1). A volume is a directory on the node's disk; local-path does not enforce a
  claim's size.
- Every pod runs under the shipped seccomp profile as a `Localhost` profile, `profiles/ultragraphdb.json`, which the script
  installs on every node through `container exec -i`; under it the roles run on io_uring, under `RuntimeDefault` on the polling
  driver (`notes/group-0.md`, 2).

### D6. The cluster runs the images Bazel builds, loaded into the shared image store

Each role's `image_index` already offers an `oci_tarball` output group, the whole multi-arch index as an OCI layout tar. The
script builds them with `-c opt` and runs `container image load -i` on each, which stores each platform's manifest as its own
untagged image; it reads the arm64 manifest's digest from the tar's `index.json` and tags it with the name the manifests use
(`container image tag untagged@sha256:… ultragraphdb/<role>:<tag>`). The cluster is created with `--shared-image-store`, so nodes
take the images from the local store; `imagePullPolicy: Never` makes a missing image an error rather than a pull. Probed on the
compute and the development image (`notes/group-0.md`, 3).

Two behaviours of `container` 1.7.1 the script works around (`notes/group-5.md`): it refuses to load an image its store already
holds ("File exists"), so an unchanged image is only tagged; and a node's containerd keeps the image it first resolved a name to,
never reading the store's tag again, so on every `up` the script drops each node's record of the images (`crictl rmi`) before the
workloads restart, and each pod resolves the name afresh.

Alternatives:
- *`container build` with a Containerfile* (the owner's suggestion; it builds multi-platform images without Docker). It would be
  a second definition of each image, its base, user and files, beside `rules_img`'s, and the cluster would test images that do
  not ship. Not needed: `container image load` takes `rules_img`'s tarball.
- *A registry on localhost.* A node VM's loopback is not the host's; and the shared store makes a registry unnecessary.

### D7. The bench runs as a Job in the cluster

`client_bench` gets an image on the glibc base like the roles (tagged `bench`, as its binary is), and runs as a Job against the
frontend's client Service, its password from the bootstrap Secret. Measuring from inside avoids a port forward on the path, and
the cluster's networking from the host is not documented. Its JSON line is the Job's log.

Two changes to `client_bench`, found running it so: a run's names carried the process's ID to keep runs apart, and in a Job every
run is process 1, so runs rewrote one another's quads; the names now carry a draw from the system's random source. And a commit a
storage pod's kill leaves in doubt ended the run; the run now resolves it as a client does, resending with its number until it is
answered, and counts it (`in_doubt`), so its `requests` are the commits acknowledged, each once.

### D8. The script drives the cluster from inside its node, outside Bazel tests and `make test`

`scripts/cluster_local.sh up|down|bench|status`, behind `make cluster-local`, `make cluster-local-down`, `make cluster-local-bench`
and `make cluster-local-status`, runs `kubectl` inside the control-plane node through `container exec -i`,
against `/etc/kubernetes/admin.conf`, the chart rendered by `helm template` on the host and passed on standard input. It so needs
no `kubectl` on the host and no network path from
the host to the cluster, which this Mac lacks until macOS's Local Network permission is granted to `container-runtime-linux`
(apple/container #1702, #2067; `notes/group-0.md`, 4). It creates the cluster with `KUBECONFIG` naming a file of its own, since
`container cluster create` merges the cluster's context into whatever file `KUBECONFIG` names. It needs the host's `container`,
and `helm`, which a hermetic, sandboxed Bazel test cannot use and CI hosts will not have, so it checks for each and names it when
missing, and
it stays outside `make test`. Images are built by Bazel; the script only runs `bazel build` for them. Node CPUs and memory are its
arguments, stated in what it reports (the default node is 4 CPUs and 32 GB here).

### D9. How the batching is measured

12.5b's matrix through the binaries, one partition and four, DirectWrites at 1, 16 and 64 connections and transactions at 16,
three interleaved 20 s runs, medians and ranges, on a `container` Linux machine and once on the `container` cluster through the
Job. The baseline is what `openspec/notes/UltraGraphDB Shard-per-Core Write Path.md` records (owner, 2026-11-09): *Stage 5.5 on
the VM disk (2026-11-08)*, its *Through the three binaries with the real client* and *A commit's phases through the binaries
(experiment X1)*, the same matrix and workload. Those ran on the OrbStack VM's disk; the runs here state their machine, its disk
and its sync time beside every table, so a difference of machine is read apart from the change. Syncs per commit come from the
partition's `commit phases` log (commits over log syncs). Acceptance: fewer syncs than commits at 16 connections and above, and
commits per second at 16 connections on one partition above the note's range (287 to 1,123); the figures, not a target, decide
what follows.

The implied overhead of the roles is read against what the commit path reaches with no roles in its way: the note's in-process
`wal_bench` with N partitions in one process (*One partition against stage 5*, the side-by-side table), at 12 partitions 125,391
relations a second at one relation per commit (124,057 to 129,360) and 185,630 at ten (179,162 to 203,082). The matrix therefore
adds 12 partitions in one storage process at 1 and 10 relations per commit, at 64 and 256 connections, and the 1- and 4-partition
points at 64 connections; each is reported as relations per second through the binaries and as the share of the in-process figure
at the same partitions and relations per commit, with the in-process range beside it. The `container` machine takes 12 CPUs, as the
OrbStack VM of the baseline had, so a partition per core is what both measured.

A share far below the in-process figure is a software defect to find and fix, not the environment's cost (owner, 2026-11-09): the
wire protocol is zero-copy from the network frame to the log's write, each role runs one compio runtime per core with no shared
state on the path, and the roles here talk over loopback and Unix sockets in one machine of unified memory. The phase logs say where
a commit's time goes, from the client's call through the frontend's checks, the relay, the queue before the core, the core's stay
and the log's sync; a phase that grows with load, or time no phase accounts for, is chased to its cause, profiled with the storage
role's symbol build where the phases stop short, and either fixed in this change or brought to the owner with its figures.

### D10. The reply path is measured on its own

Once a round answers many commits at once, the partition writes their replies in a burst on the frontend's one role connection,
and the frontend reads them through `RoleClient`'s pool of one buffer, one frame at a time, each relayed to its client before the
next is read. Whether that keeps up is measured, not assumed:

- The frontend already times each one-frame write from its relay to its partition's answer (`OneFramePhases`). The partition gains
  the matching interval: from `StorageSession` reading the request to its reply's push being taken by the connection to be written,
  in a window of its own beside `commit phases`, logged as `role connection phases`. Both intervals are read on one process's clock
  each, so no clock crosses a process.
- Per run, the residual, the frontend's relay-to-answer less the partition's read-to-reply, is the two transits and the time a reply
  waits to be read at the frontend. Its median and 99th percentile are reported against the round's size, commits per sync, from
  the same window.
- The reply path keeps up when the residual at the largest rounds measured stays within the residual's range at one commit per
  round, 12.5b's case, measured in the same series with one connection. A residual growing with the round's size is replies waiting
  at the frontend's reader, and goes to the owner with its figures before any change to the frontend.
- A burst test pins the reader alone: a partition answering 1,000 one-frame writes in one round over one role connection, read back
  one frame at a time through `RoleClient`, the frontend's role link's reader, in process; the time from the sync's release to the
  first answer and from the first to the last. The frontend's relay to its clients is the phases' part, under load in group 6.

### D11. The scenarios hold a real sync

Group 1 wrote the scenarios against a tap that holds the core's inbox, standing in for a turn waiting on its log's sync, because
`StorageHost` fixes the log's storage to `TimedStorage<CompioStorage>` and a test cannot hold a sync of it. `PartitionCore`,
`CoreParts` and `StorageHost` take the log's storage as a type parameter, `S: DurableStorage`, which the storage role names as
`CompioStorage`, monomorphised with no cost at run time. A test then opens the log over a storage wrapping the real one, whose file
syncs it can hold, and the scenarios run with a sync truly in flight: the core's turn waits in `PartitionHost::turn` while the
connections hand on what arrives, and the round after it is formed from them. The tap stays to count what reaches the inbox. The
measurements of group 6 run the role binaries unchanged.

### D12. A turn's intake closes one round (owner's decision, 2026-11-09)

The core takes its inbox only between turns, after a sync, so the commits that arrived during the sync reach the commit step one by
one at a moment with no round in flight. `CommitStep::arrive` closes a round at once for a commit arriving so, as `commit-rounds`
states for a lone commit on an idle partition, so the first commit of every backlog synced alone and the rest shared the next round:
two syncs per backlog, measured in group 3 with a real sync held (three rounds where two were due). The commit step gains
`Input::Hold` and `Input::Release`: while held, a commit arriving with no round in flight waits; the release closes one round over
what waits, when none is in flight. `Pipeline::hold_rounds` and `release_rounds`, and `PartitionHost`'s, pass them on, and
`PartitionCore::run` holds around each turn's intake from its inbox. A lone commit on an idle partition still closes its round in
the turn that takes it. Alternatives: the core taking its inbox while the sync is in flight (the non-goal above), or two syncs per
backlog left as they were; the owner chose this one.

### D13. Performance keeping the guarantees, then the storage engine

With commits batching through the roles, the measurements put the cost of a commit in the storage engine's work per relation and
in allocation, not in strict serializability: stage 5's commit thread spent 20–25 µs a relation, about seven LSM memtable
operations, and 12.5's profile 47% of it reading referrer sets from the store; 12.5b counted 176–258 allocations per relation; the
resolver's validation appears in no profile's top entries. Group 7 removes what the KV store does not force, each step keeping
strict serializability and the sync of every round before its acknowledgement, and measured alone and together against group 6:
the referrer reads on the write path, allocation per relation, the index entries the layout can merge, and the owner's epoch-close
sync on the commit path. 7.1 first divides the commit thread's time, validation's share among it, so each step's expected gain is
known before it is built. Each step is kept only on evidence (owner, 2026-11-09): higher transaction throughput outside the runs'
spread keeps and finishes it, in turn; anything else takes it out again, with its figures recorded. What remains after the four is the KV store's floor, the measured case for roadmap stage 6.5, a tile-based
graph storage engine: KV stores suit a graph's adjacency poorly, and building on one served the architecture until now
(owner, 2026-11-09).

### D14. The frontend serves clients on every core (owner's decision, 2026-11-09)

Group 6 found the frontend's one endpoint runtime the cluster's ceiling: 98.8% of a core relaying every client's frames, about
27 µs a one-frame write, near 37,000 single-relation commits a second whatever the partitions or the clients, while twelve
partition cores ran at 23% each (`notes/group-6.md`, 6.4). `role-binaries` has each role run one runtime per core it uses; the
frontend used one for its whole client path. The owner chose to fix it in this change.

- The frontend runs an endpoint core per CPU the process may use, as compute runs a compute core per CPU: core 0 on the service's
  runtime, each other on a thread of its own with its runtime (`core_runtime`, pinned when `pin_cores` asks). The client listener
  is bound once and each core accepts on a handle of the same socket (`CoreListener`, moved from `ultragraphdb_common_compute` to
  `ultragraphdb_common_service` so the frontend need not depend on compute's crate); the kernel spreads connections over them,
  and a connection's requests stay on its core.
- Each core holds what its sessions share on one thread: its `EndpointCore`, its `RoleLinks` with the placement map read from the
  owner at its start, its tenant gate, and the phases of the writes it relays, logged with its core's number. Each core dials its
  own role link to each partition and compute process, so a partition takes one role connection per frontend core.
- The role listener, the connections back and readiness stay on core 0. A `Join` reaches every other core's `RoleLinks` by
  message, an unbounded channel per core drained by a task of that core, so every core routes to what has joined.
- Every core stops accepting when the service stops, and its runtime stays up until the open requests a stop waits on are
  answered.
- The failed-authentication counts stay one for the frontend, in its owner thread (D15).
- A core's thread is spawned as compute's and storage's are, with the same `clippy::disallowed_methods` expectation and reason.
- **The target (owner, 2026-11-09):** as close to the in-process 127,695 relations a second at 12 partitions as the roles can come,
  measured like for like: storage alone on 12 CPUs, as `wal_bench`'s partitions had them, and the frontend, its owner, compute and
  the clients on CPUs of their own, over the network; the figure with every role on one 12-CPU machine is reported beside it.
  The CPU per commit measured at 33,649 commits a second, about 81 µs on the partitions, 29 µs on the frontend's endpoint thread
  and 3 µs on its owner, puts one 12-CPU machine near 106,000 commits a second at best, so the shared figure also waits on group 7.
  Spreading the frontend over cores lifts its one thread's cap; its 29 µs a commit is then profiled (a symbol build of the frontend,
  as storage has) and cut, each change kept on its figures. `client_bench` is one thread, so a run toward the target uses several.
- **The layout on this Mac (owner, 2026-11-09):** its 16 cores hold no 12 for storage beside the frontend's, so the comparison is
  made at 6 partitions: storage alone on a 6-CPU `container` machine, the frontend with its owner, the config service and compute
  on another of 6, the clients on a third of 2, two cores left to macOS (`--cpus 5` and `--cpus 1`, as `container` gives one CPU
  more than asked). The target is `wal_bench`'s rate at 6 partitions on the storage machine itself, its CPUs and disk.

### D15. Failed logins: throttled, then banned, in the owner (owner's decision, 2026-11-09)

Spreading the frontend over cores (D14) split the throttle of failed authentications, which lived on the one endpoint core: per
core, a source's doubling delay restarted on each fresh core and the lockout of a name needed its failures on every core. The owner
chose to keep the counts in the owner thread, where every core's logins already ask for the name's verifier, and a new posture:

- **Per source**, an IPv4 address or an IPv6 /64 (one attacker holds a whole /64): consecutive failures counted, a success clearing
  them. The first `[endpoint] auth_failures_before_throttle` (50) wait nothing; the next `auth_throttled_failures_before_ban` (20)
  each wait a delay doubling from 100 ms to at most 30 s; then the source is banned for `auth_ban_ms` (24 hours), each of its
  connections closed as accepted, before TLS or any frame. The ban ends on its own and clears the count, as a source behind a shared
  address (a NAT, a load balancer rewriting source addresses) may hold other clients. Each setting is at least 1.
- **No lockout per principal**, and `refusal_interval_ms` is gone with it.
- **By message**: the verifier's ask carries the source and answers the delay with the verifier; the session reports each
  authentication's outcome without waiting for an answer; each accepted connection asks whether its source is banned on its own
  task, so accepting never waits on the owner. Times travel in the messages from the asking core's clock.
- **Bounded**: at most 65,536 sources; a full table drops sources counting failures before it would drop a ban, and with only bans
  left a new source goes uncounted. In memory: a restart forgets the counts; several frontends (stage 8) would share them through
  the metadata group.

## Risks / Trade-offs

- [The frontend reads a storage connection's replies one at a time (`RoleClient`'s pool of one buffer), and a round now answers
  many commits at once] → measured in D10; if the replies queue at the frontend, the owner decides the fix with the figures.
- [A copy per request] → a commit frame is a few KiB, copied once; the parked path already copies, and now moves its copy instead
  of copying again.
- [Answers reordered by the move to one channel] → the core is the one writer of the channel; scenarios pin per-stream order,
  an Abort ending with no frame, a refusal after a pushed frame, and a Cancel crossing a request.
- [Larger rounds lengthen the apply, and a lone commit's latency] → a round closes at once when no sync is in progress, so a lone
  commit waits one sync as before; the round bounds cap the apply.
- [`container cluster` is experimental and its flags may change] → the script names the `container` version it ran with, and the
  measurements state it; nothing in `make test` depends on it.
- [The baseline ran on the OrbStack VM's disk; these runs on `container`'s] → every table states its machine and the log's sync
  time on it, so a change in the disk is read apart from the change in batching.

## Migration Plan

None: no frame, setting or stored format changes, and there are no outside users. The Kubernetes deployment is new.

## Open Questions

- Once rounds carry many commits, does the frontend's reply path keep up (D10), or is a second change needed there?
