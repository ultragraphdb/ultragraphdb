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

- [x] 2.1 `Push`, a frame or `End`; `Session::next_push` yields `(u64, Push)`; `serve_connection` ends a stream on `End` as on
  `Answer::Done`, and drops a push for a stream no longer open as today; every implementor of `Session` updated
- [x] 2.2 Tests: `End` ends the stream with no frame, a push after `End` on that stream is dropped, and the order of pushes is the
  order on the wire

## 3. The session reads on → `ultragraphdb_common_storage`

- [x] 3.1 `StorageSession::answer` copies the frame into a `ParkedFrame` counted against the connection's `ParkedBytes`, sends the
  copy to the core and returns `Answer::Open`; the `done` channel and `ending` removed; the session's documentation states the
  bound of what it sends the core and the push channel's length
- [x] 3.2 `PartitionCore` answers every request through its push channel in its order: replies, refusals as Error frames, and
  `End` for a stream ending with no frame; a parked request moves its copy into its `Parked` entry rather than copying again; the
  copy's bytes are released when the core drops it
- [x] 3.3 The partition's `role connection phases` (design D10): from `StorageSession` reading a request to its reply's push being
  taken by the connection, in a window logged once a second at most beside `commit phases`, with the commits per sync of the same
  window; tests of the window as `OneFramePhases` has them
- [x] 3.4 The burst test (design D10): in process, 1,000 one-frame writes sent on one role connection while the log's sync is
  held, answered in one round, read back one frame at a time through `RoleClient`, the frontend's role link's reader; the time from
  the sync's release to the first answer and from the first to the last, recorded, with no bound asserted until the owner sets one
- [x] 3.5 The scenarios hold a real sync (design D11): `PartitionCore`, `CoreParts` and `StorageHost` generic over the log's
  `DurableStorage`, the storage role naming `CompioStorage`; in the tests a storage wrapping the real one whose file syncs the test
  can hold; the scenarios of 1.2 and 1.3 hold the log's sync in place of the tap's hold, the tap still counting what reaches the
  inbox, and the commit-rounds scenario asserts one append and one sync for the commits sent while a real sync was in flight
- [x] 3.6 One round per intake (design D12, the owner's decision): `Input::Hold` and `Input::Release` in the commit step, passed on by
  `Pipeline` and `PartitionHost`, the core holding around each turn's intake; commit step tests of the hold, a release with nothing
  waiting and a release while a round syncs; the one-round scenario fails without the hold (three rounds) and passes with it (two)
- [x] 3.7 The scenarios of groups 1 and 2 pass; `ultragraphdb_storage_tests`, `ultragraphdb_tests` and the frontend's suites pass
  unchanged, natively and in the Linux suites under `container run` as user 65532 (`container` has no seccomp option; the shipped
  profile is exercised by the Kubernetes deployment, task 5.4)

## 4. The bench and its image → `ultragraphdb_tool_bench`

- [x] 4.1 `client_bench` in a Job: its password from the file the bootstrap Secret mounts (`--password-file`, as it was), the
  frontend's client Service dialled by its name (`--address` resolves a host's name at start, an IP as before); syncs per commit
  derived by the measurement from the partition's `commit phases` log, whose `commits` and `log_syncs` give them
- [x] 4.2 An image of `client_bench` on the glibc base, tagged `bench` as its binary is, with its `oci_tarball`

## 5. The Kubernetes deployment → `deploy/helm/ultragraphdb`, `ultragraphdb_config`

- [x] 5.1 The `kube` profile in `ultragraphdb/ultragraphdb_config/profiles/`: the config service's file, `targets.toml` and the role
  files, one storage instance and one partition, addresses the headless Services' names; validated by the config service's checks
  and its CLI in a test, as the other profiles are
- [x] 5.2 The manifests and their `kustomization.yaml`: namespace, the config service, frontend and storage StatefulSets with a
  volume each, compute as a Deployment, headless Services publishing not-ready addresses, the client Service, the profile's
  ConfigMap, the Secrets' names (no values), the bootstrap variables, the probes, the security context, `imagePullPolicy: Never`,
  and the seccomp profile as 0.2 decided; the bench Job
- [x] 5.3 `scripts/cluster_local.sh up|down|bench|status` behind the make targets `cluster-local`, `cluster-local-down`,
  `cluster-local-bench` and `cluster-local-status`, listed in `make help`; `deploy/kubernetes/` holds no script: checks for `container`
  and names it when missing; builds the images'
  `oci_tarball`s with `-c opt`, loads them and tags each arm64 manifest by its digest; creates the cluster with
  `--shared-image-store`, its node CPUs and memory as arguments and `KUBECONFIG` naming a file of its own; applies kind's
  `default-storage.yaml` and installs the seccomp profile on every node; draws the keys and secrets from `/dev/urandom` into
  Secrets; applies the manifests and waits for every role's readiness, all through `kubectl` inside the control-plane node; reports
  the driver each role runs on, the node sizes and the `container` version; `down` deletes the cluster
- [x] 5.4 The kubernetes-deployment scenarios run by hand on this Mac and recorded with their output in `notes/group-5.md`: up
  and down; a pod runs unprivileged; a storage pod restarts at its address; a recovering storage pod is alive and unready; a storage
  pod killed under writes keeps every acknowledged commit; a missing role secret stops a role with code 78; no secret under
  `deploy/kubernetes/`; a missing tool is named; the roles run on io_uring; a bench Job reports
- [x] 5.5 `AGENTS.md`: the deployment, the script, the `kube` profile and the bench image; `bazel/README_BAZEL.md`: the
  `oci_tarball` output group as the path into `container`
- [x] 5.6 The Helm chart `deploy/helm/ultragraphdb` in place of the plain manifests (the owner, 2026-11-09): the workloads, the
  Services, the `kube` profile moved into the chart and its ConfigMap, the Secrets drawn at the first install and kept, the bench
  Job; `scripts/cluster_local.sh` renders it with `helm template` and applies it in the node, making the Secrets only where absent;
  the config battery checks the chart's profile; recorded in `notes/group-5.md`

## 6. Measurements

- [x] 6.1 Design D9 on a `container` Linux machine against the baseline the Shard-per-Core note records (*Stage 5.5 on the VM
  disk*, *Through the three binaries with the real client*, *A commit's phases through the binaries (experiment X1)*): the role
  binaries built with `-c opt`, one partition and four, DirectWrites at 1, 16 and 64 connections and transactions at 16, three
  interleaved 20 s runs, medians and ranges, the loads, the machine and the log's sync time stated; syncs per commit and the phases
  from the `commit phases` and `one-frame write phases` logs, beside the note's tables; plus 12 partitions in one storage process at
  1 and 10 relations per commit and 64 and 256 connections, on a machine of 12 CPUs, each point reported as the share of the note's
  in-process rate at the same partitions and relations per commit (at 12: 124,057–129,360 and 179,162–203,082 relations a second),
  the roles' implied overhead
- [x] 6.2 The same matrix on the `container` cluster through the bench Job, one storage instance, the driver and node sizes stated
- [x] 6.3 The reply path (design D10), in the series of 6.1 and 6.2: per run the residual, the frontend's relay-to-answer less the
  partition's read-to-reply, its median and 99th percentile against commits per sync, beside the one-connection runs' residual at
  one commit per round; the burst test's figures; whether the reply path keeps up, and, if not, the figures to the owner
- [x] 6.6 The frontend serves clients on every core (design D14): `CoreListener` moved to `ultragraphdb_common_service`; an
  endpoint core per CPU, core 0 keeping the role listener, the connections back and readiness; `Join` passed to every core;
  each core's map, links, gate and phases (logged with its core); every core stops accepting on a stop; the authentication
  throttle one for the frontend
- [x] 6.7 The frontend battery: writes over many connections answered with several endpoint cores serving and each core's runtime
  logged; a partition joining after the start reached from every core; failed authentications counted across cores; a stop
  refusing new connections on every core; the `role-binaries` requirement stating it
- [x] 6.8 Like for like (design D14): storage alone on a 6-CPU `container` machine, the frontend, its owner, the config service and
  compute on another of 6, the clients (several `client_bench` processes) on a third of 2, over the network; 6 partitions at 1 and
  10 relations a commit against `wal_bench` at 6 partitions on the storage machine, the threads' time on a CPU, and the 12-CPU
  shared figure beside it
- [x] 6.10 Failed logins throttled, then banned, in the owner (design D15): the three `[endpoint]` settings in place of
  `refusal_interval_ms`; the owner's counts per source (IPv4 address, IPv6 /64); the verifier's ask answering the delay, each
  outcome reported, each accepted connection admitted or closed; tests of the posture and of a banned source's connection closed,
  another source served; `SECURITY.md` and the client-endpoint requirement
- [x] 6.11 A partition executes the next round while its log syncs (design D16): `OrderedLog::detach_sync` and `attach_sync`, the
  WAL's, the pipeline's and the host's; `PartitionCore::run` waiting on its sync and its inbox together; tests of a detached sync,
  of requests executed during a held sync, and of a stop during one; measured as 6.8, kept on its figures. Measured 7% to 17%
  below inline syncs, half the commits a sync: reverted (`notes/group-6.md`)
- [x] 6.9 The frontend's cost a commit profiled (`ultragraphdb_frontend_profile`, a symbol build as storage's) and cut, each change
  kept only where 6.8's figures rise outside their spread. Profiled; no cut: the frontend bounds neither figure, its checks 4 to 5%
  of a commit's latency; the owner closed it (`notes/group-6.md`)
- [x] 6.4 Any share of the in-process rate far below it chased to its cause (design D9): the phase that grows or the time no phase
  accounts for, profiled with the symbol builds where the phases stop short, and fixed here or brought to the owner with its figures
- [x] 6.5 The results in `notes/group-6.md`, the Shard-per-Core note beside X1's figures, and backlog item 18's *Batching* bullet
  closed with what this change built and measured

## 7. Performance keeping the guarantees

Each improvement keeps strict serializability and the sync of every round before its acknowledgement; each is measured alone and
together against group 6's figures, on the same machine and matrix, as relations per second and as the share of the in-process
ceiling (design D13). The rule is evidence (owner, 2026-11-09): an improvement whose figures show higher transaction throughput,
outside the runs' spread, is kept and finished in turn, its tests, spec deltas and documentation with it; one that does not is
taken out again, and its figures are recorded with the reason.

- [x] 7.1 The baseline split: `perf` with symbols (`wal_bench_profile`, `ultragraphdb_storage_profile`) of the commit thread, in
  process at 1 and 12 partitions and through the binaries at 16 connections, its time divided among execution, validation of the
  read sets (what strict serializability costs), the commit step's decision, the log's write and sync, apply and allocation, at 1,
  10 and 50 relations per commit; the core's per-relation time through the binaries (X1: about 63 µs) set against the in-process
  20–25 µs, the difference explained
- [x] 7.2 Referrer reads off the write path: `referrer_candidates` (`ultragraphdb_store_apply`) no longer reads a hub's referrer set
  from the store on each commit, by caching it within the partition or by skipping the read for a bucket group the round already
  names, whichever keeps "A commit under a registration reply reads its mark" and the referrer sets' completeness (`partitions`);
  tests first; measured in process at 1 and 12 partitions over 3 s and 30 s runs (the decay from 35,984 to 14,202 relations a
  second gone) and through the binaries
- [x] 7.3 Allocation per round, not per relation: the terms, changesets and records between a DirectWrite's frame and its round
  built in buffers the pipeline lends and takes back, as its read sets are; `alloc_bench`'s allocations per term and per relation
  before and after (12.5b: 52–86 a term, 176–258 a relation), and the throughput. Measured, not built: allocation is 5 to 8.5% of the commit
  thread, no caller above 0.6%; the owner closed it (`notes/group-7.md`)
- [x] 7.4 Index entries per relation within the KV layout: the keys a relation writes counted by keyspace and each keyspace's share
  of the commit thread's time; entries the layout can merge or drop without changing what any read returns or what recovery
  rebuilds are merged or dropped, tests first; what only a new storage format can remove is recorded as input to roadmap stage 6.5
  (the tile-based storage engine). Counted: 9.1 versions a relation, 5.0 of them
  the reads'; the object-predicate SETs and the statistics deltas change reads to remove, recorded for stage 6.5, nothing merged
  (the owner; `notes/group-7.md`)
- [x] 7.5 The owner's epoch-close sync off the commit path (backlog item 18, *The owner's sync*): the owner answers asks that read
  nothing unsynced ahead of its sync, or the endpoint holds a tenant's facts between closes, whichever keeps the gate's and the
  owner check's guarantees (`client-endpoint`, `metadata-group`); spec deltas for the behaviour that changes, tests first; the
  frontend's checks and a one-relation commit measured at the `dev` epochs against X1 (1.44 ms and 3.93 ms). Built as design D17, the owner answering a commit's checks during its syncs: one
  connection +6%, the checks' p99 1.9 to 0.3 ms; the rate under load unchanged (`notes/group-7.md`)
- [x] 7.6 The four together, group 6's matrix again: each improvement's gain alone and together, the share of the in-process ceiling
  at 1, 4 and 12 partitions, validation's share of the commit thread, and the per-relation floor the KV store leaves, in
  `notes/group-7.md` and the Shard-per-Core note. Measured at `6753d3ad`: +2.6% to +21.8% over group 6, the shares 51 to 109% at six
  partitions; `openspec/benches/benches-2026-11-09.md`
- [x] 7.7 What remains for roadmap stage 6.5: the storage engine's floor per relation and per commit after the four, by keyspace and
  operation, recorded in the backlog as the measured case for the tile-based storage engine. Recorded: 42 to 44 µs of CPU a relation in the KV store, 16 µs a commit on the
  commit thread (backlog item 19)

## 8. Verification

- [x] 8.1 Ultra Style on every changed crate; `make format` and `make fix` leave the tree unchanged; `cargo clippy --workspace
  --all-targets -- -D warnings` clean
- [x] 8.2 Mutation testing of every crate this change touches (`ultragraphdb_common_role`, `ultragraphdb_common_storage`,
  `ultragraphdb_transactions_commit` and those group 7 changes) over its diff, as the group's
  last step; survivors killed or recorded with their reason
- [x] 8.3 `make test` green with OrbStack started, so the supervisor's Docker image tests run, and the five Linux suites under
  `container run` as user 65532; the final counts recorded
- [x] 8.4 Cleanup once the measurements are recorded, each listed in `notes/group-8.md` before it is removed: every `container`
  cluster this change created (`container cluster delete`), the images it loaded into `container`'s store (`ultragraphdb/*`, the
  bench's, the untagged platform images, `busybox`, and `kindest/node` unless the owner keeps it), `container` volumes and the
  probe's leftovers from group 0, the script's kubeconfig and state files, and the data volumes and binary directories the
  measurements left on the `container` machine; `container image list`, `container cluster list` and `container volume list` show none of
  them afterwards
