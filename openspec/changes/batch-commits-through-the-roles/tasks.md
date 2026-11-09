## 0. The local cluster's unknowns

- [x] 0.1 On this Mac with `container` 1.7.1: create a throwaway cluster with `--shared-image-store` and record in
  `notes/group-0.md`, each with the command and its output: whether a default StorageClass exists and binds a PersistentVolumeClaim
  (and, if none, the provisioner the manifests will use); whether `container exec` or `container cp` reaches a node so a file can be
  placed under `/var/lib/kubelet/seccomp/`; whether `container image load -i` takes a `rules_img` `oci_tarball` of one role's index
  and under which name, and whether a pod with `imagePullPolicy: Never` starts from it; how a host process reaches a Service, if at
  all; then delete the cluster
- [x] 0.2 Answer the design's open questions from 0.1 in `design.md`, moving each to a decision, and bring any question the
  findings raise to the owner before group 5

## 1. Protocol and scenarios first

- [x] 1.1 `PROTOCOL.md` §11.12, *Parked requests*: a partition copies every request out of the receive buffer and reads on without
  waiting for its core; held bytes count those copies with the parked ones against `storage.held_bytes`
- [x] 1.2 Scenarios in the storage tests (a partition core whose inbox the test holds, as a turn waiting on its log's sync holds it,
  through `StorageSession` and `serve_connection`): wire-protocol "A connection is read on while its core syncs", "Requests waiting for the core count against
  the bound", "An Abort's stream ends with no frame"; commit-rounds "Commits a frontend sends during a sync share the next round";
  each written first and seen failing against the current code
- [x] 1.3 The order scenarios the move to one channel can break, written first: a transaction's Writes and Commit waiting for the
  core answered in the order sent; a Cancel crossing a request waiting for the core; a connection closing with a Write waiting for
  the core aborts its transaction. Covered where they stand and kept passing: a request parked on a home among a stream's requests
  and a Cancel or a close reaching a parked write (`ultragraphdb_storage_tests`, `parked_requests.feature`, task 3.5), and a
  refusal crossing its stream's end (`test_a_refusal_crossing_its_streams_end_gives_way_to_it`, adapted to the one channel in
  group 3)

## 2. One ordered push channel → `ultragraphdb_common_role`

- [ ] 2.1 `Push`, a frame or `End`; `Session::next_push` yields `(u64, Push)`; `serve_connection` ends a stream on `End` as on
  `Answer::Done`, and drops a push for a stream no longer open as today; every implementor of `Session` updated
- [ ] 2.2 Tests: `End` ends the stream with no frame, a push after `End` on that stream is dropped, and the order of pushes is the
  order on the wire

## 3. The session reads on → `ultragraphdb_common_storage`

- [ ] 3.1 `StorageSession::answer` copies the frame into a `ParkedFrame` counted against the connection's `ParkedBytes`, sends the
  copy to the core and returns `Answer::Open`; the `done` channel and `ending` removed; the session's documentation states the
  bound of what it sends the core and the push channel's length
- [ ] 3.2 `PartitionCore` answers every request through its push channel in its order: replies, refusals as Error frames, and
  `End` for a stream ending with no frame; a parked request moves its copy into its `Parked` entry rather than copying again; the
  copy's bytes are released when the core drops it
- [ ] 3.3 The partition's `role connection phases` (design D10): from `StorageSession` reading a request to its reply's push being
  taken by the connection, in a window logged once a second at most beside `commit phases`, with the commits per sync of the same
  window; tests of the window as `OneFramePhases` has them
- [ ] 3.4 The burst test (design D10): in process, a partition core answering 1,000 queued one-frame writes in one round over one
  role connection to a frontend endpoint, the time from the first reply leaving to the last answer reaching its client connection,
  against the partition's time to write them; recorded, with no bound asserted until the owner sets one
- [ ] 3.5 The scenarios of groups 1 and 2 pass; `ultragraphdb_storage_tests`, `ultragraphdb_tests` and the frontend's suites pass
  unchanged, natively and in the Linux suites under the shipped seccomp profile

## 4. The bench and its image → `ultragraphdb_tool_bench`

- [ ] 4.1 `client_bench` takes its password from a file or the environment a Job's Secret provides, and reports the partition's
  syncs per commit when given the phase log's figures, or the measurement script derives them from the log
- [ ] 4.2 An image of `client_bench` on the glibc base, tagged `bench` as its binary is, with its `oci_tarball`

## 5. The Kubernetes deployment → `deploy/kubernetes/`, `ultragraphdb_config`

- [ ] 5.1 The `kube` profile in `ultragraphdb/ultragraphdb_config/profiles/`: the config service's file, `targets.toml` and the role
  files, one storage instance and one partition, addresses the headless Services' names; validated by the config service's checks
  and its CLI in a test, as the other profiles are
- [ ] 5.2 The manifests and their `kustomization.yaml`: namespace, the config service, frontend and storage StatefulSets with a
  volume each, compute as a Deployment, headless Services publishing not-ready addresses, the client Service, the profile's
  ConfigMap, the Secrets' names (no values), the bootstrap variables, the probes, the security context, `imagePullPolicy: Never`,
  and the seccomp profile as 0.2 decided; the bench Job
- [ ] 5.3 `deploy/kubernetes/local.sh up|down|bench|status`: checks for `container` and names it when missing; builds the images'
  `oci_tarball`s with `-c opt`, loads them and tags each arm64 manifest by its digest; creates the cluster with
  `--shared-image-store`, its node CPUs and memory as arguments and `KUBECONFIG` naming a file of its own; applies kind's
  `default-storage.yaml` and installs the seccomp profile on every node; draws the keys and secrets from `/dev/urandom` into
  Secrets; applies the manifests and waits for every role's readiness, all through `kubectl` inside the control-plane node; reports
  the driver each role runs on, the node sizes and the `container` version; `down` deletes the cluster
- [ ] 5.4 The kubernetes-deployment scenarios run by hand on this Mac and recorded with their output in `notes/group-5.md`: up
  and down; a pod runs unprivileged; a storage pod restarts at its address; a recovering storage pod is alive and unready; a storage
  pod killed under writes keeps every acknowledged commit; a missing role secret stops a role with code 78; no secret under
  `deploy/kubernetes/`; a missing tool is named; the roles run on io_uring; a bench Job reports
- [ ] 5.5 `AGENTS.md`: the deployment, the script, the `kube` profile and the bench image; `bazel/README_BAZEL.md`: the
  `oci_tarball` output group as the path into `container`

## 6. Measurements

- [ ] 6.1 Design D9 on the OrbStack VM, like for like with 12.5b: the role binaries built with `-c opt`, one partition and four,
  DirectWrites at 1, 16 and 64 connections and transactions at 16, three interleaved 20 s runs, medians and ranges, the loads
  stated; syncs per commit and the phases from the `commit phases` and `one-frame write phases` logs; against 12.5b's tables
- [ ] 6.2 The same matrix on the `container` cluster through the bench Job, one storage instance, the driver and node sizes stated
- [ ] 6.3 The reply path (design D10), in the series of 6.1 and 6.2: per run the residual, the frontend's relay-to-answer less the
  partition's read-to-reply, its median and 99th percentile against commits per sync, beside the one-connection runs' residual at
  one commit per round; the burst test's figures; whether the reply path keeps up, and, if not, the figures to the owner
- [ ] 6.4 The results in `notes/group-6.md`, the Shard-per-Core note beside X1's figures, and backlog item 18's *Batching* bullet
  closed with what this change built and measured

## 7. Verification

- [ ] 7.1 Ultra Style on every changed crate; `make format` and `make fix` leave the tree unchanged; `cargo clippy --workspace
  --all-targets -- -D warnings` clean
- [ ] 7.2 Mutation testing of `ultragraphdb_common_role` and `ultragraphdb_common_storage` over this change's diff, as the group's
  last step; survivors killed or recorded with their reason
- [ ] 7.3 `make test` green and the five Linux suites under the shipped seccomp profile; the final counts recorded
- [ ] 7.4 Cleanup once the measurements are recorded, each listed in `notes/group-7.md` before it is removed: every `container`
  cluster this change created (`container cluster delete`), the images it loaded into `container`'s store (`ultragraphdb/*`, the
  bench's, the untagged platform images, `busybox`, and `kindest/node` unless the owner keeps it), `container` volumes and the
  probe's leftovers from group 0, the scripts' kubeconfig and state files, and the data and binary directories the measurements left
  on the OrbStack VM (`/var/tmp/<run>`); `container image list`, `container cluster list` and `container volume list` show none of
  them afterwards
