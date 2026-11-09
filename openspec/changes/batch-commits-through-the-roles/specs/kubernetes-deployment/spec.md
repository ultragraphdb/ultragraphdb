## ADDED Requirements

### Requirement: The roles deploy on Kubernetes from the repository's manifests
The repository SHALL hold, under `deploy/kubernetes/`, the manifests that deploy one cluster of the roles in one namespace: the
config service, the frontend and each storage instance as a StatefulSet of one replica with a volume of its own for its data
directory, compute as a Deployment, and a headless Service per StatefulSet and for compute. The frontend and the metadata group
and sequencer it hosts SHALL run as one replica, as they hold one replica in this stage. Every pod SHALL run as user 65532, with
no privilege escalation, every capability dropped and a read-only root filesystem, writing only to its volume. Applying the
manifests to an empty namespace SHALL bring every role to `/ready` with no step by hand.

#### Scenario: An empty namespace comes up
- **WHEN** the manifests are applied to an empty namespace of a cluster holding the images
- **THEN** the config service, the frontend, every storage instance and compute each answer `/ready`, and a client reaching the
  frontend's client endpoint authenticates and commits

#### Scenario: A pod runs unprivileged
- **WHEN** any role's pod is inspected
- **THEN** it runs as user 65532 with a read-only root filesystem, no capability and no privilege escalation

### Requirement: Role addresses are stable names
Each role's role address SHALL be its pod's stable DNS name under its headless Service, which the role binds and the peers dial, so
an address survives a pod's restart on another IP. Each headless Service SHALL publish not-ready addresses, so a role resolves and
binds its own name before it is ready. Storage and compute SHALL reach the frontend at its Service's name, which the config service
accepts beside the frontend's own (`config-service`, the *Frontend* rule).

#### Scenario: A storage pod restarts at its address
- **WHEN** a storage pod is deleted and its StatefulSet starts it again with another IP
- **THEN** it joins the frontend at the same role address, claims the same partitions, and the frontend's next request to it is
  answered

### Requirement: Settings, keys and secrets reach the roles as Kubernetes objects
The deployment's settings SHALL be a profile the config service validates, its targets and role files held in a ConfigMap the
config service mounts. The config service's read and admin keys, the role secret and the bootstrap secret SHALL be Secrets, each
mounted as a file only into the pods that read it. Each role SHALL receive its five bootstrap variables from its pod's environment
(`config-service`, "A role starts from five bootstrap variables") and nothing else from it. The keys and secrets SHALL be drawn at
random when the deployment is first created and never written into the repository.

#### Scenario: A missing role secret stops a role
- **WHEN** the role secret's Secret is absent and a storage pod starts
- **THEN** the pod exits with code 78 naming the role secret's file, and starts once the Secret exists

#### Scenario: No secret is in the repository
- **WHEN** the files under `deploy/kubernetes/` are searched
- **THEN** none holds a key, a secret or a password

### Requirement: The probes follow each role's health and readiness
Each pod SHALL be probed on its role's observability address: the startup and liveness probes on `/health`, the readiness probe on
`/ready`, so a storage pod recovering its log stays alive and unready, and a pod whose role stops answering `/health` is restarted.

#### Scenario: A recovering storage pod is alive and unready
- **WHEN** a storage pod restarts over a log with rounds to replay
- **THEN** it is not restarted during the replay, is unready until the replay ends, and is ready after it

### Requirement: A pod's data survives its restart
A role's data directory SHALL be its volume, so every commit acknowledged before a pod is killed SHALL read back after the pod
starts again over the same volume.

#### Scenario: A storage pod killed under writes keeps every acknowledged commit
- **WHEN** a storage pod is deleted with a grace period of zero while a client commits through the frontend, and the pod starts
  again
- **THEN** every commit acknowledged before the deletion reads back once the pod is ready

### Requirement: The deployment runs on a local Apple container cluster
`deploy/kubernetes/` SHALL hold the manifests alone. The repository SHALL hold `scripts/cluster_local.sh`, run through `make
cluster-local` (create and deploy), `make cluster-local-down`, `make cluster-local-bench` and `make cluster-local-status`, that, on a Mac
with Apple silicon and Apple's `container` with its `cluster` command, creates a
local cluster whose nodes share the local image store (`container cluster create --shared-image-store`), loads into that store the
role images Bazel builds for `linux/arm64` and the bench's image, applies the manifests, waits until every role is ready, and
deletes the cluster on request. It SHALL drive the cluster from inside its control-plane node, needing no `kubectl` on the host
and no network path from the host to the cluster, and SHALL write no kubeconfig file of the owner's. It SHALL pull no image of the
roles from a registry and push none to one. It SHALL refuse to run, naming what is missing, without `container`. The script and its
runs SHALL stay outside `make test`.

#### Scenario: Up and down
- **WHEN** `make cluster-local` and then `make cluster-local-down` run
- **THEN** every role answers `/ready` after the first run, and after the second no cluster of that name is listed

#### Scenario: A missing tool is named
- **WHEN** the script runs where `container` is not on the path
- **THEN** it exits non-zero naming `container`, and creates nothing

### Requirement: The pods run under the shipped seccomp profile
The script SHALL install the shipped seccomp profile, `bazel/oci_images/seccomp/ultragraphdb.json`, on every node of the cluster,
and every role's pod SHALL name it as a `Localhost` profile, so the roles run on io_uring; each role logs the I/O driver it started
on, so a run states what it measured.

#### Scenario: The roles run on io_uring
- **WHEN** the deployment is up
- **THEN** every node holds the profile byte for byte, and each role's log says its runtimes started on the io_uring driver

### Requirement: The bench runs inside the cluster
`client_bench` SHALL run as a Kubernetes Job in the deployment's namespace against the frontend's client Service, so a measurement
crosses no port forward from the host, and its result SHALL be the Job's log, one JSON line as on a host.

#### Scenario: A bench Job reports
- **WHEN** a bench Job runs DirectWrites for 20 s at 16 connections
- **THEN** its log holds one JSON line with the commits, their rate and their latency, and the Job completes
