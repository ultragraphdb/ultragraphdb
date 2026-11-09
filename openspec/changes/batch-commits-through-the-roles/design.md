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
- The batching measured through the binaries against 12.5b's figures, on the cluster and like for like on the OrbStack VM.

**Non-Goals:**
- Taking the inbox while a sync is in flight (overlapping the sync with the next round's decisions): batching does not need
  it, and it means splitting `Pipeline::sync` so the log's sync no longer borrows the pipeline. Measured first, decided after.
- The other open items of backlog item 18 (the owner's epoch-close sync, referrer-set reads, a Scan turn's cost), each its own
  question.
- A production Kubernetes deployment: TLS between roles (stage 8), more than one frontend, an operator, Helm charts, amd64
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
order is the order on the wire. `Session::next_push` yields `(stream, Push)` where `Push` is a frame or `End`, a stream ending
with no frame (an Abort's `Answer::Done` today), and `serve` ends the stream on `End` as it does on `Answer::Done`. A refusal is
built into its Error frame by the core, as `push_error` already does for a parked request. The `done` channel and `ending` go.
`serve` already drops a push for a stream no longer open.

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
- A headless Service per workload gives the stable names the role addresses use; `publishNotReadyAddresses: true` lets a role
  resolve and bind its own name before it is ready (role addresses accept host names, resolved as the connection opens, and 12.6
  ran the roles bound to their containers' names).
- A `kube` profile beside `dev` and `dev-net` in `ultragraphdb/ultragraphdb_config/profiles/`, its addresses the Services'
  names, validated by the config service and its CLI like the others. The profile lives in a ConfigMap; keys and secrets are
  Secrets the script draws from `/dev/urandom` at first creation.
- Plain manifests with a `kustomization.yaml` (`kubectl apply -k`, built into `kubectl`): no Helm, no operator. Pods run as user
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

Alternatives:
- *`container build` with a Containerfile* (the owner's suggestion; it builds multi-platform images without Docker). It would be
  a second definition of each image, its base, user and files, beside `rules_img`'s, and the cluster would test images that do
  not ship. Not needed: `container image load` takes `rules_img`'s tarball.
- *A registry on localhost.* A node VM's loopback is not the host's; and the shared store makes a registry unnecessary.

### D7. The bench runs as a Job in the cluster

`client_bench` gets an image on the glibc base like the roles (tagged `bench`, as its binary is), and runs as a Job against the
frontend's client Service, its password from the bootstrap Secret. Measuring from inside avoids a port forward on the path, and
the cluster's networking from the host is not documented. Its JSON line is the Job's log.

### D8. The script drives the cluster from inside its node, outside Bazel tests and `make test`

`deploy/kubernetes/local.sh up|down|bench|status` runs `kubectl` inside the control-plane node through `container exec -i`,
against `/etc/kubernetes/admin.conf`, manifests on standard input. It so needs no `kubectl` on the host and no network path from
the host to the cluster, which this Mac lacks until macOS's Local Network permission is granted to `container-runtime-linux`
(apple/container #1702, #2067; `notes/group-0.md`, 4). It creates the cluster with `KUBECONFIG` naming a file of its own, since
`container cluster create` merges the cluster's context into whatever file `KUBECONFIG` names. It needs the host's `container`,
which a hermetic, sandboxed Bazel test cannot use and CI hosts will not have, so it checks for it and names it when missing, and
it stays outside `make test`. Images are built by Bazel; the script only runs `bazel build` for them. Node CPUs and memory are its
arguments, stated in what it reports (the default node is 4 CPUs and 32 GB here).

### D9. How the batching is measured

12.5b's matrix through the binaries, one partition and four, DirectWrites at 1, 16 and 64 connections and transactions at 16,
three interleaved 20 s runs, medians and ranges: once on the OrbStack VM as 12.5b ran it, like for like with its figures, and
once on the `container` cluster through the Job. Syncs per commit come from the partition's `commit phases` log (commits over
log syncs). Acceptance: fewer syncs than commits at 16 connections and above, and commits per second at 16 connections on one
partition above 12.5b's range (287 to 1,123); the figures, not a target, decide what follows.

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
- A burst test pins the same thing without load: a partition answering 1,000 queued one-frame writes in one round over one role
  connection to a frontend endpoint, in process, the time from the first reply leaving to the last answer reaching its client
  connection reported against the partition's own time to write them.

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
- [The node VMs' disk and CPU differ from the OrbStack VM's] → D9 measures on both, and compares like with like.

## Migration Plan

None: no frame, setting or stored format changes, and there are no outside users. The Kubernetes deployment is new.

## Open Questions

- Once rounds carry many commits, does the frontend's reply path keep up (D10), or is a second change needed there?
