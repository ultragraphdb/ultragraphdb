## MODIFIED Requirements

### Requirement: The roles deploy on Kubernetes from the repository's Helm chart
The repository SHALL hold, at `deploy/helm/ultragraphdb`, the Helm chart that deploys one cluster of the roles in one
namespace: the config service and each storage instance as a StatefulSet of one replica with a volume of its own for its
data directory, each compute instance as a StatefulSet of one replica with no volume, as an instance takes its name and
its settings file from its pod's environment, under a headless Service that gives its pod the stable name it advertises
to clients, a headless Service per StatefulSet, and a Service over every compute pod for a client's seeds. The first
storage instance SHALL host the system group, its logs on that instance's volume; a cluster MAY hold several compute
instances, each with its own settings file and its own StatefulSet, each a node of the node list (node-discovery), so
scaling out is adding an instance and a settings revision; the system group holds one replica until stage 8. Every pod
SHALL run as its image's user, 65532 for a role and 65534 for the config service (`config-service`, "One config service
holds every role's settings"), with no privilege escalation, every capability dropped and a read-only root filesystem,
writing only to its volume. Installing the chart into an empty namespace SHALL bring every role to `/ready` with no step
by hand; its values SHALL set the images and their pull policy, the cluster's domain, the volumes' class and size, the
seccomp profile, whether the chart makes the Secrets, and the bench Job.

#### Scenario: An empty namespace comes up
- **WHEN** the chart is installed into an empty namespace of a cluster holding the images
- **THEN** the config service, every storage instance and every compute pod each answer `/ready`, no compute pod mounts a volume, and a client reaching compute's client Service authenticates, receives the node list and commits

#### Scenario: A pod runs unprivileged
- **WHEN** any role's pod is inspected
- **THEN** it runs as its image's user, 65532 for a role and 65534 for the config service, with a read-only root filesystem, no
  capability and no privilege escalation

### Requirement: Role addresses are stable names
Each role's role address SHALL be its pod's stable DNS name under its headless Service, which the role binds and the
peers dial, so an address survives a pod's restart on another IP. Each headless Service SHALL publish not-ready
addresses, so a role resolves and binds its own name before it is ready. Every role SHALL reach the system group at the
hosting storage pod's stable name, which the config service accepts beside the address that pod binds (`config-service`,
the *System address* rule). Compute has no role address: nothing dials it but clients, at the client Service for the
first contact and then at each pod's stable name, which the pod advertises (node-discovery).

#### Scenario: A storage pod restarts at its address
- **WHEN** a storage pod is deleted and its StatefulSet starts it again with another IP
- **THEN** it joins the system group from the same role address, claims the same partitions, and compute's next request to it is
  answered

### Requirement: Settings, keys and secrets reach the roles as Kubernetes objects
The deployment's settings SHALL be the chart's `kube` profile, which the config service and its CLI validate, its
targets and role files held in a ConfigMap the config service mounts, their addresses naming the release's namespace and
cluster domain. The config service's read and admin keys, the role secret and the bootstrap secret SHALL be Secrets,
each mounted as a file only into the pods that read it: the bootstrap secret into the storage pod hosting the system
group, and into no compute pod. Each role SHALL receive its five bootstrap variables from its pod's environment
(`config-service`, "A role starts from five bootstrap variables") and nothing else from it. The chart SHALL draw the
keys and secrets at random at the first install, keep each through every upgrade and an uninstall, and make none when
told the Secrets exist; none SHALL be written into the repository.

#### Scenario: A missing role secret stops a role
- **WHEN** the role secret's Secret is absent and a storage pod starts
- **THEN** the pod exits with code 78 naming the role secret's file, and starts once the Secret exists

#### Scenario: An upgrade keeps the Secrets
- **WHEN** the chart is installed and then upgraded, or rendered and applied again by the script
- **THEN** each Secret keeps the value it was given first, and the bootstrap administrator still logs in

#### Scenario: Compute's pod holds no bootstrap secret
- **WHEN** compute's pod is inspected
- **THEN** it mounts the role secret and no bootstrap secret, and holds no volume claim

#### Scenario: No secret is in the repository
- **WHEN** the files under `deploy/helm/` are searched
- **THEN** none holds a key, a secret or a password

### Requirement: A pod's data survives its restart
A storage instance's data directory, and the system group's in the instance hosting it, SHALL be its volume, so every
commit acknowledged before a pod is killed SHALL read back after the pod starts again over the same volume, and compute
SHALL hold nothing a restart could lose.

#### Scenario: A storage pod killed under writes keeps every acknowledged commit
- **WHEN** a storage pod is deleted with a grace period of zero while a client commits through compute, and the pod starts
  again
- **THEN** every commit acknowledged before the deletion reads back once the pod is ready

#### Scenario: The hosting storage pod killed keeps the books
- **WHEN** the storage pod hosting the system group is deleted with a grace period of zero after a session was registered and a
  catalog version committed, and the pod starts again
- **THEN** the session's next commit is accepted and the catalog version is still the tenant's

#### Scenario: A deleted compute pod loses nothing
- **WHEN** compute's pod is deleted and its Deployment starts another
- **THEN** a client reconnects, authenticates, and commits through the session it registered before

### Requirement: The bench runs inside the cluster
`client_bench` SHALL run as a Kubernetes Job of the chart, in the deployment's namespace against compute's client
Service, so a measurement crosses no port forward from the host, and its result SHALL be the Job's log, one JSON line as
on a host.

#### Scenario: A bench Job reports
- **WHEN** a bench Job runs DirectWrites for 20 s at 16 connections
- **THEN** its log holds one JSON line with the commits, their rate and their latency, and the Job completes
