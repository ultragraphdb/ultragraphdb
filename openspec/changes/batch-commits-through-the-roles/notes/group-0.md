# Group 0: the local cluster's unknowns

Probed on 2026-11-09, 03:46 to 04:00 UTC, on this Mac: macOS 27.2 (26B123), Apple silicon, 16 CPUs; `container` 1.7.1 (release,
commit `a15b46097aa4`), its `cluster` command experimental. One throwaway cluster, `ugdb-probe`, created with
`container cluster create --name ugdb-probe --shared-image-store`, deleted at the end. The logs of every step are in the session's
scratchpad, `g0/`.

## The cluster

- Created in 3 min 27 s, most of it pulling `docker.io/kindest/node:v1.35.5` on first use. One node, `control-plane,worker`, 4 CPUs
  and 32 GB (a quarter of the host, the default), at 192.168.64.2, its API published on the host at 127.0.0.1:6445.
- The node: Kubernetes v1.35.5, Debian GNU/Linux 13, kernel 6.18.56 aarch64, containerd 2.3.1, kindnet as CNI, CoreDNS.
- `container cluster create` merged the cluster's context into the file `KUBECONFIG` names, here
  `~/.kube/carverauto-oidc.yaml`, not `~/.kube/config`; `container cluster delete` removed it again.
  `container cluster write-config --name ugdb-probe --kubeconfig <file>` writes a file holding the context but setting no current
  context, so `kubectl` needs `--context ugdb-probe` with it.
- Deleting the cluster took 6 s.

## 1. Volumes: no StorageClass until kind's local-path manifest is applied

```text
$ container exec ugdb-probe sh -c 'kubectl --kubeconfig /etc/kubernetes/admin.conf get storageclass'
No resources found
```

The node image carries kind's own manifests, `/kind/manifests/default-cni.yaml` and `/kind/manifests/default-storage.yaml`, and
its two images (`docker.io/kindest/local-path-provisioner:v20260521-9fb22683`, `docker.io/kindest/local-path-helper:v20260131-7181c60a`)
are preloaded; `container cluster create` applies the CNI and not the storage. Applied from the node:

```text
$ container exec ugdb-probe sh -c 'KUBECONFIG=/etc/kubernetes/admin.conf kubectl apply -f /kind/manifests/default-storage.yaml'
storageclass.storage.k8s.io/standard created
NAME                 PROVISIONER             RECLAIMPOLICY   VOLUMEBINDINGMODE      ALLOWVOLUMEEXPANSION
standard (default)   rancher.io/local-path   Delete          WaitForFirstConsumer   false
```

A claim of 1 Gi bound once its pod was scheduled, and the pod wrote, synced and read back a file. The volume is a directory under
`/var/local-path-provisioner/` on the node's root filesystem, `/dev/vdb`, ext4, 504 GB, a sparse disk image of the node; local-path
does not enforce the claim's size.

## 2. The seccomp profile: installed through `container exec`; under it the roles run on io_uring

`container exec` reaches a node as root, with standard input:

```text
$ container exec -i ugdb-probe sh -c 'mkdir -p /var/lib/kubelet/seccomp/profiles && cat > /var/lib/kubelet/seccomp/profiles/ultragraphdb.json' \
    < bazel/oci_images/seccomp/ultragraphdb.json
2496f069c72cdac7eb18cd9b5101c3c731545868bcb5f10d183d3c0efaef9e33  /var/lib/kubelet/seccomp/profiles/ultragraphdb.json   (the node)
2496f069c72cdac7eb18cd9b5101c3c731545868bcb5f10d183d3c0efaef9e33  bazel/oci_images/seccomp/ultragraphdb.json            (the repository)
```

The development image (`-c opt`, `ultragraphdb_supervisor:development_multiarch`, its `dev` profile, every role in one pod, user
65532) ran in three pods side by side, each reaching every role's `/ready`:

| Pod's `securityContext.seccompProfile` | Driver of every runtime |
|---|---|
| none (the pod runs Unconfined) | io_uring |
| `RuntimeDefault` (containerd 2.3.1's default) | polling |
| `Localhost`, `profiles/ultragraphdb.json` | io_uring |

## 3. Images: Bazel's `oci_tarball` loads; each platform is stored apart and is tagged by its digest

```text
$ bazel build -c opt //ultragraphdb/ultragraphdb_compute:ultragraphdb_compute_multiarch --output_groups=oci_tarball
  target-bzl/bin/ultragraphdb/ultragraphdb_compute/ultragraphdb_compute_multiarch_oci_layout.tar      (11,956,224 bytes)
$ container image load -i …/ultragraphdb_compute_multiarch_oci_layout.tar                             (0.29 s)
untagged@sha256:c188d14576bb9a5a4b8a34145727c0c7e76ce6a0ee76918e2039f0cf97dd810c
untagged@sha256:a0915696afa61a6615fa00d885deef11c4e18ff480b0896d951a7e51bb2206f8
```

The tar's `index.json` is the image index itself, listing the amd64 and the arm64 manifest and naming no reference, so `container`
stores each manifest as its own untagged image, wrapped in a one-platform index. Tagging the arm64 manifest names it:

```text
$ container image tag untagged@sha256:a0915696…bb2206f8 ultragraphdb/compute:probe
$ container image inspect ultragraphdb/compute:probe    # name docker.io/ultragraphdb/compute:probe, one variant, arm64 v8, user 65532
```

A pod naming `ultragraphdb/compute:probe` started from the shared store under `imagePullPolicy: Never` and under `IfNotPresent`
alike ("already present on machine"), with no registry; the binary ran and exited with code 78, "configuration: the variable
ULTRAGRAPHDB_CONFIG_URL is required", as it does without its bootstrap variables. Once a pod used it, the node's containerd listed
it (`crictl images`: `docker.io/ultragraphdb/compute probe`, 5.46 MB). The development image (40,442,368 bytes) loaded and ran the
same way.

## 4. Reaching the cluster from the host: not possible on this Mac; inside the cluster, names and Services work

From this session's processes the host reaches neither a node nor a container:

| From the host | Result |
|---|---|
| `kubectl` to the API's published port, 127.0.0.1:6445 | TCP connects, then every request is reset |
| `kubectl` to the node, 192.168.64.2:6443 | `connect: no route to host` |
| `ping 192.168.64.2` | 100% loss; `arp` holds no entry for it, though the route goes over `bridge101` (192.168.64.1/24) |
| a plain container (`busybox` httpd) on its published port, `-p 127.0.0.1:18080:8080` | reset (`curl` exit 56) |
| that container at its own address, 192.168.64.3 | unreachable |

`container`'s own log names the cause, the same line for the cluster's API and for the plain container:

```text
container-runtime-linux: [com.apple.container:RuntimeLinuxHelper] backend - connect failed: connect(descriptor:addr:size:):
  No route to host) (errno: 65) [uuid=ugdb-probe] [proxy=[IPv4]127.0.0.1/127.0.0.1:6445] …
container-runtime-linux: … backend - connect failed: … No route to host) (errno: 65) [uuid=ugdb-portprobe] [proxy=… :18080] …
```

It is a known issue of the host's settings, not of `container`: apple/container #1702 (host cannot reach container IPs, published
ports reset; `container` 1.0.0, macOS 26.3.1) and #2067 (the forwarder's backend connect fails with errno 65; 1.2.0, macOS 26.6)
both closed as macOS's *Local Network* privacy permission not granted to `container-runtime-linux`. The fix that closed both:
enable `container-runtime-linux` under System Settings, Privacy & Security, Local Network, then `container stop --all`,
`container system stop`, `container system start`. #2029 adds that macOS 27 betas before beta 7 (26A5421a) did not list the
helper in that panel at all, and #2442 (open) that each update of `container` adds another `container-runtime-linux` entry there,
so the enabled one may not be the current binary's. `bridge100` on this Mac is OrbStack's network (192.168.139.3/23), not a second
`container` network, so #2051 (two networks on one bridge) does not apply.

Inside the cluster every path the design needs works, driven from the host through `container exec` into the control-plane node,
whose `kubectl` reads `/etc/kubernetes/admin.conf`:

- `kubectl apply -f -` takes manifests on standard input through `container exec -i`.
- A headless Service with `publishNotReadyAddresses: true` and a pod with `hostname: dev-0`, `subdomain: devsvc`: from another pod,
  `dev-0.devsvc.default.svc.cluster.local` resolved to the pod (10.244.0.12) and its client port 7080 accepted a connection.

## What the design takes from this

1. Volumes: the script applies `/kind/manifests/default-storage.yaml` from the node after creating the cluster; the StatefulSets'
   claims use the `standard` class, on the node's disk.
2. Seccomp: the script installs the shipped profile on every node through `container exec -i`, and every pod runs under it as a
   `Localhost` profile, so the roles run on io_uring as they would under the shipped profile elsewhere.
3. Images: the script loads each role's `oci_tarball`, reads the arm64 manifest's digest from the tar's `index.json`, and tags it
   with the name the manifests use; `imagePullPolicy: Never`.
4. Control: the script runs `kubectl` inside the control-plane node through `container exec`, so it needs no `kubectl` on the host
   and no network path from the host to the cluster, and it creates the cluster with `KUBECONFIG` pointing at a file of its own,
   so it leaves the owner's kubeconfig files untouched. Measurements run inside the cluster (design D7).
5. Node size: the default node is 4 CPUs and 32 GB; the script states the size it creates and takes it as an argument.

Left in the local image store by the probe: `ultragraphdb/compute:probe`, `ultragraphdb/development:probe`, `busybox:1.37.0`, four
untagged images and `kindest/node`. The owner decides whether to remove them (`container image delete`).
