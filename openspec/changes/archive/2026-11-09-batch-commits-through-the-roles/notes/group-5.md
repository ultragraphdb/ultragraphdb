# Group 5: the Kubernetes deployment

Run on 2026-11-09, 06:17 to 06:36 UTC, on this Mac: macOS 27.2, Apple silicon, 16 CPUs; `container` 1.7.1 (release, commit
`a15b460`); the cluster `ultragraphdb` created by `make cluster-local` with 12 CPUs and 16 GB asked for, one node,
`control-plane,worker`, Kubernetes v1.35.5. The node reports 13 CPUs where 12 were asked for. The logs are in the session's
scratchpad, `g5/`.

## 5.1 The `kube` profile

`ultragraphdb/ultragraphdb_config/profiles/kube.toml` and `kube/{targets,frontend,storage,compute}.toml`: `dev-net`'s settings,
each role address its pod's name under its headless Service (`frontend-0.frontend.ultragraphdb.svc.cluster.local:7081`,
`storage-0.storage…:7100`, `compute.compute…:7082`), storage and compute dialling the frontend at its Service's name
(`frontend.ultragraphdb.svc.cluster.local:7081`, which the Frontend rule accepts, as a host may differ), the keys and secrets under
`/secrets`, every observability address and the client endpoint on `0.0.0.0` so the kubelet's probes and the client Service reach
them. `ultragraphdb_config_cli` passes it ("target kube: compute, frontend, storage"); the config battery's profile scenario now
checks `dev`, `dev-net` and `kube` (44 of 44). The `config-service` delta names `kube` among the shipped profiles.

## 5.2 The manifests

`deploy/kubernetes/`: `kustomization.yaml`, `namespace.yaml`, `config.yaml`, `frontend.yaml`, `storage.yaml`, `compute.yaml`, and
`bench/job.yaml` applied apart per run. Departures from the first draft, recorded in design D5 and the spec:

- The config service runs as 65534, its image's user, as `config-service` states; the roles as 65532.
- Compute is a Deployment whose pod sets `hostname: compute` and `subdomain: compute`, so its role address is a stable name as a
  StatefulSet pod's is; the kubelet writes it into the pod's `/etc/hosts`, so compute binds it at once. `Recreate`, one replica.
- Every Secret is mounted `optional`, so a missing one is a role's exit 78 naming the file, not a pod waiting unstarted.
- Every container has `terminationMessagePolicy: FallbackToLogsOnError`, so a failed start's last log lines stay in the pod's status.

## 5.3 The script

`scripts/cluster_local.sh up|bench|status|down` behind `make cluster-local` (`CPUS=12 MEMORY=16g`), `cluster-local-bench`
(`BENCH=`, by default `--op direct --connections 16 --seconds 20`), `cluster-local-status` and `cluster-local-down`, listed in
`make help`. Two behaviours of `container` 1.7.1 found on the second `up`, over a cluster already running:

1. `container image load` of an image the store already holds fails: `NSCocoaErrorDomain Code=516 … couldn't be copied … because
   an item with the same name already exists`. The script loads only an image whose arm64 digest the store does not hold
   (`container image inspect untagged@<digest>`) and tags either way.
2. The node's containerd keeps the image it first resolved `ultragraphdb/client_bench:local` to (`crictl images`: image ID
   `f1c220045a37a`) after the tag moved, so a restarted pod ran the old binary. `crictl rmi` of a name drops containerd's record,
   also while a pod runs it; the next pod resolves the name afresh (`1902c83bce7cf`). `up` drops every node's record of the five
   images before restarting the workloads.

## 5.4 The scenarios

| Scenario | Result |
|---|---|
| Up and down | `make cluster-local` from nothing: 72 s (images built), every rollout complete, the four pods ready 9 s after apply. `make cluster-local-down`: 7 s, `container cluster list` empty. The owner's `KUBECONFIG` (`~/.kube/carverauto-oidc.yaml`) and `~/.kube/config` hold no mention of the cluster; its kubeconfig is `target/cluster_local/kubeconfig`. A second `up` over the running cluster redeploys and restarts the workloads |
| A pod runs unprivileged | Each pod: `runAsNonRoot`, read-only root, `allowPrivilegeEscalation: false`, capabilities dropped `ALL`, seccomp `profiles/ultragraphdb.json`. Each process on the node (`/proc/<pid>/status`): compute, frontend and storage `Uid 65532`, config `Uid 65534`; `CapEff 0000000000000000`, `NoNewPrivs 1`, `Seccomp 2` (filter) |
| The roles run on io_uring | The profile on the node: `2496f069…efaef9e33`, the repository's byte for byte. Runtimes: compute 13 on io_uring (the node's 13 CPUs), frontend 2, storage 2; the config service starts no compio runtime and logs none |
| A bench Job reports | `--op direct --connections 16 --seconds 20`, one partition: 183,367 commits, **9,168 a second**, p50 1,579 µs, p99 2,834 µs, 0 refused. Storage's `commit phases`: 24,825 log syncs, **7.4 commits a sync**, sync p50 287 µs; before this change one partition peaked near 1,300 commits a second, one sync each (12.5b). Measurement proper is group 6 |
| A storage pod restarts at its address | `storage-0` force-deleted at 06:29:03.8 under 16 writing connections: Pending at once, running at 10.244.0.36 (was .31) at 06:29:05, ready at 06:29:06, restarts 0. The frontend reached it at its name; the run went on |
| A storage pod killed under writes keeps every acknowledged commit | A scan before: 183,367 relations. The 30 s run, storage killed 12 s in: 149,803 commits acknowledged, 16 left in doubt and resolved by resend, 0 refused. A scan after: **333,170 = 183,367 + 149,803**: every acknowledged commit read back, none applied twice |
| A recovering storage pod is alive and unready | Alive and unready 1 to 2 s, then ready, no restart, on both kills. Both recoveries replayed **0 rounds** (`recovered partition=1 rounds=0`): a process killed leaves its unsynced store writes in the node's page cache, so the store is current. A replay needs the node itself to lose memory; the long replay is covered by the probes' design (startup probe on `/health`, 90 × 2 s) and not observed here |
| A missing role secret stops a role | `ultragraphdb-role-secret` deleted, `storage-0` deleted: exit 78, the status message `configuration: the role secret file /secrets/role.secret: No such file or directory (os error 2)`. The Secret made again with its value: ready 14 s later, after two restarts |
| No secret under `deploy/kubernetes/` | No `data:`, `stringData`, Secret object, PEM block or 32-character base64 run |
| A missing tool is named | `PATH=/usr/bin:/bin bash scripts/cluster_local.sh up`: exit 1, "needs \`container\` on the path: Apple's container, 1.7 or later, from https://github.com/apple/container"; no cluster listed after |

### What the scenarios found in `client_bench`

The first kill run counted nothing, and the scans did not grow:

- A run's names carried `{seed}-{pid}`. In a Job every run is process 1, so every run wrote the same subjects `…/1-1/c<n>/n<k>`,
  and a quad written twice is one quad: the scans counted the largest run's commits, 183,367, whatever ran after. The names now
  carry a draw from the system's random source (`system_nonce`).
- A commit left in doubt by the kill (`refused (Unavailable, detail 0): the role connection closed under the request`) ended the
  run. The run now resolves it as a client does, `Client::resolve` with its number every 100 ms for at most a minute, counts it in
  `in_doubt`, and goes on; a commit refused on resolution counts as refused.

### Open: a scan after a restart

A scan of every `w` relation (`--op scan`, one connection) takes 0.49 s for 183,367 rows on a warm cluster, 18 s right after the
roles restart, 27 s for 333,170 rows right after the kill, and 4.3 s for the same rows a minute later. Storage's `role connection
phases` during an 18 s scan: 60 requests, read-to-answer p50 156 ms. Not on the commit path this change measures; recorded for the
read-path work.

## 5.5 Documentation

`AGENTS.md`: the make targets and *Local Kubernetes* (the manifests, the `kube` profile, the bench Job, what `up`, `bench`, `status`
and `down` do). `bazel/README_BAZEL.md`: the `oci_tarball` output group as the path into `container`.

## 5.6 The Helm chart (the owner, 2026-11-09)

`deploy/helm/ultragraphdb` replaces the manifests and `kustomization.yaml` of 5.2, which became its templates: the workloads and
Services through shared helpers (labels, pod and container security, the five bootstrap variables, the probes, the volume claim),
`profile.yaml`, `secrets.yaml`, `bench-job.yaml` behind `bench.enabled`, and `NOTES.txt`. The `kube` profile moved into the chart,
`profile/`, the one copy: the ConfigMap carries it with `.ultragraphdb.svc.cluster.local` replaced by the release's namespace and
domain (rendered into the namespace `graphs` under `example.internal`, every address names them), and the config battery reads it
from there (`//deploy/helm/ultragraphdb:profile`, 44 of 44). `helm lint` passes. Values: the images and pull policy, the cluster
domain, the volumes' class and size, the seccomp profile, `secrets.create`, the bench.

The Secrets: `randAlphaNum 64` at the first install, read back by `lookup` on an upgrade, `helm.sh/resource-policy: keep`.
`helm template` reads no cluster, so the script renders them apart and creates only those absent. `helm install` and `helm upgrade`
against a cluster were not run: this Mac's host reaches no `container` cluster (group 0, 4) and the node holds no `helm`; `lookup`
is Helm's documented behaviour, not observed here.

Runs on this Mac:

- Over the cluster the manifests had made, `apply` refused the three StatefulSets: their claim templates named `storageClassName:
  standard` and the chart's name the default class, a field a StatefulSet cannot change. A cluster first made by the chart does not
  meet it; the cluster was made again.
- `make cluster-local` from nothing: 72 s, the four Secrets created, every rollout complete, compute 13 runtimes on io_uring,
  frontend 2, storage 2. A second `make cluster-local`: no Secret created, the four Secrets' UIDs unchanged, the workloads restarted.
- `make cluster-local-bench` after it, the bootstrap password as the first install drew it: 190,566 commits in 20 s, 9,528 a
  second, p50 1,509 µs, p99 2,722 µs, 0 refused, 0 in doubt.
- `up` and `bench` with `container` and `bazel` on the path and no `helm`: exit 1, "needs \`helm\` on the path: Helm 3 or later,
  from https://helm.sh".

## The suites

`cargo fmt --all -- --check` and `cargo clippy -p ultragraphdb_tool_bench -p ultragraphdb_config_tests --all-targets -D warnings`
clean. `make test` (OrbStack running): exit 0; doc 21 of 21, unit 79 of 79, BDD 37 of 37, integration 7 of 7, the supervisor's
`image_bdd` among them; again after 5.6, exit 0, the same counts. The cluster `ultragraphdb` stays up for group 6.
