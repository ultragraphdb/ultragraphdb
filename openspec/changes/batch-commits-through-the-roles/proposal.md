## Why

Through the three binaries every commit syncs the log alone: the commit step groups the commits waiting when a sync
ends into one round (`commit-rounds`, "Commits arriving during a sync share the next round"), but a partition's role
connection hands its core one request at a time and reads the next frame only once the core has taken the last, and the
core takes its inbox only between syncs. The frontend sends every client's writes for a partition over one role
connection, so at most one commit waits when a sync ends. Stage 5.5's experiment X1 measured it: one log sync per commit
in every run, 17.2 ms of a 19.9 ms commit queued before the core at 16 connections, and one partition capped near 1,300
commits a second with no thread busy, against 11,881 to 14,235 relations a second in process, where the rounds batch
(`notes/group-12.md` of the archived stage 5.5 change, 12.5b part 2; backlog item 18, *Batching*).

The roles have also run only as processes, in the development image and on one Docker host. Kubernetes is a decided
deployment target (backlog item 1), and the roles' addresses, settings, secrets, probes and images were shaped for it,
but no deployment exists and nothing has run on one. Apple's `container` runtime (1.7.1 on this Mac) creates local
Kubernetes clusters of lightweight VMs, so the batching can be measured, and the roles tested, on a real Kubernetes
deployment without a remote cluster.

## What Changes

- A partition's role connection hands each request to its core as it arrives and reads on, without waiting for the core
  to take it. Every answer reaches the connection as a push, in the order the core gives them. The frames handed on and
  not yet taken count against `storage.held_bytes`, as parked frames do, so the connection's bound is unchanged.
- What waits in a partition's inbox when a sync ends forms the next round: commits sent over one role connection during
  a sync share one log append and one sync, as the commit step already requires of commits arriving during a sync.
- `PROTOCOL.md` §11.12, *Parked requests*, states that a partition reads a role connection's next frame before its core
  has taken the last, and counts those frames against `held_bytes`. No frame and no command changes on the wire.
- A Kubernetes deployment of the roles under `deploy/kubernetes/`: the config service, the frontend and storage as
  StatefulSets with a volume each, compute as a Deployment, a headless Service per role for stable role addresses, the
  role secret as a Secret, and the probes on `/livez` and `/readyz`.
- The deployment runs on a local Apple `container` cluster: the images built by Bazel are loaded into the local image
  store, which the cluster's nodes share, and a client bench runs as a Job inside the cluster.
- The batching is measured through the binaries on that cluster, against 12.5b's figures, with the syncs per commit
  counted from the partition's phase log.

## Capabilities

### New Capabilities
- `kubernetes-deployment`: the roles deployed on Kubernetes (workloads, Services, volumes, the role secret, settings
  delivery, probes and the seccomp profile) and run on a local Apple `container` cluster from the images Bazel builds.

### Modified Capabilities
- `wire-protocol`: "A parked request holds no role connection" covers the frames a partition holds unanswered as well as
  parked: a role connection is read on before its core takes a request, and every answer is pushed in the core's order.
- `commit-rounds`: "Commits are grouped into rounds" holds through a role connection: commits a frontend sends over one
  connection while a sync is in progress share the next round.

## Impact

- `ultragraphdb_common_storage`: `StorageSession` sends each request on and answers `Open`, counting the frame against its
  connection's `ParkedBytes` until the core takes it; `PartitionCore` pushes every answer, ending a stream with no frame
  by a push of its own; the `done` channel goes.
- `ultragraphdb_common_role`: `Session::next_push` carries a stream's end with no frame as well as a frame, and `serve`
  ends the stream on it, as it does for `Answer::Done`.
- `specification/binary_protocol/PROTOCOL.md` §11.12.
- New `deploy/kubernetes/` (manifests), Bazel targets exporting each role's image as an OCI tarball for
  `container image load`, and a script that creates the cluster, loads the images, deploys and tears down.
- `ultragraphdb_tool_bench`: `client_bench` runs in a Job, so its image is built as the roles' are.
- Tests: the storage BDD suite gains the batching scenarios; the Kubernetes runs are by hand on a Mac with Apple silicon
  and `container`, outside `make test`, since they need the host's `container` and `kubectl`.
