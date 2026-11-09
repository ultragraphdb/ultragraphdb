#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

# The roles on a local Kubernetes cluster of Apple's `container` (macOS on Apple silicon), deployed from the Helm chart
# `deploy/helm/ultragraphdb`:
#
#   cluster_local.sh up [--cpus N] [--memory SIZE] [--partitions N]
#                                                     creates the cluster when absent, builds and loads the images, deploys, waits;
#                                                     a node of N CPUs, the storage instance holding N partitions
#   cluster_local.sh bench [client_bench arguments]   runs client_bench as a Job inside the cluster and prints its JSON line
#   cluster_local.sh status                           the cluster, its nodes, the pods and the I/O driver each role runs on
#   cluster_local.sh down                             deletes the cluster
#
# Run through `make cluster-local`, `make cluster-local-bench`, `make cluster-local-status` and `make cluster-local-down`. Helm
# renders the chart on the host (`helm template`) and every kubectl runs inside the control-plane node, so the host needs no
# kubectl and no network path into the cluster. The cluster's kubeconfig is its own file under `target/cluster_local/`, never the
# owner's. No image is pulled from or pushed to a registry.

set -o errexit
set -o nounset
set -o pipefail

CLUSTER="ultragraphdb"
NAMESPACE="ultragraphdb"
REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
STATE="${REPO}/target/cluster_local"
CHART="deploy/helm/ultragraphdb"
SECCOMP="bazel/oci_images/seccomp/ultragraphdb.json"
DEFAULT_BENCH=(--op direct --connections 16 --seconds 20)

# Each image: its Bazel target, then the name the chart runs it by.
IMAGES=(
  "//ultragraphdb/ultragraphdb_config:ultragraphdb_config_multiarch ultragraphdb/config:local"
  "//ultragraphdb/ultragraphdb_frontend:ultragraphdb_frontend_multiarch ultragraphdb/frontend:local"
  "//ultragraphdb/ultragraphdb_storage:ultragraphdb_storage_multiarch ultragraphdb/storage:local"
  "//ultragraphdb/ultragraphdb_compute:ultragraphdb_compute_multiarch ultragraphdb/compute:local"
  "//ultragraphdb_tool/ultragraphdb_tool_bench:client_bench_multiarch ultragraphdb/client_bench:local"
)
WORKLOADS=(statefulset/config statefulset/frontend statefulset/storage deployment/compute)

fail() {
  echo "cluster_local.sh: $*" >&2
  exit 1
}

needs() {
  command -v "$1" >/dev/null 2>&1 || fail "needs \`$1\` on the path: $2"
}

# Whether the cluster exists.
exists() {
  container cluster list 2>/dev/null | awk -v name="${CLUSTER}" '$1 == name { found = 1 } END { exit !found }'
}

# kubectl inside the control-plane node, standard input passed on.
kube() {
  container exec -i "${CLUSTER}" env KUBECONFIG=/etc/kubernetes/admin.conf kubectl "$@"
}

# The chart rendered as the release `ultragraphdb` in the namespace, pulling no image, with `$@` set.
render() {
  helm template "${CLUSTER}" "${REPO}/${CHART}" --namespace "${NAMESPACE}" --set imagePullPolicy=Never "$@"
}

# A shell command inside the control-plane node, kubectl configured.
node_sh() {
  container exec -i "${CLUSTER}" env KUBECONFIG=/etc/kubernetes/admin.conf sh -c "$1"
}

# The digest of the arm64 manifest an OCI layout tar's index names.
arm64_digest() {
  tar -x -O -f "$1" index.json | tr ',{}' '\n\n\n' | awk -F'"' '
    $2 == "digest" { digest = $4 }
    $2 == "architecture" && $4 == "arm64" { print digest; exit }'
}

build_and_load() {
  local targets=() entry
  for entry in "${IMAGES[@]}"; do
    targets+=("${entry%% *}")
  done
  echo "Building the images (-c opt)"
  (cd "${REPO}" && bazel build -c opt "${targets[@]}" --output_groups=oci_tarball)
  local bin
  bin="$(cd "${REPO}" && bazel info -c opt bazel-bin 2>/dev/null)"
  for entry in "${IMAGES[@]}"; do
    local target="${entry%% *}" name="${entry##* }"
    local package="${target#//}"
    package="${package%%:*}"
    local tarball="${bin}/${package}/${target##*:}_oci_layout.tar"
    [ -f "${tarball}" ] || fail "Bazel built no ${tarball}"
    local digest
    digest="$(arm64_digest "${tarball}")"
    [ -n "${digest}" ] || fail "${tarball} names no arm64 manifest"
    # `container` 1.7.1 refuses to load an image its store already holds ("File exists"), so an unchanged image is only tagged.
    if ! container image inspect "untagged@${digest}" >/dev/null 2>&1; then
      container image load -i "${tarball}" >/dev/null
    fi
    container image tag "untagged@${digest}" "${name}"
    echo "Loaded ${name} (${digest})"
  done
}

create_cluster() {
  local cpus="$1" memory="$2"
  mkdir -p "${STATE}"
  echo "Creating the cluster ${CLUSTER}: ${cpus} CPUs, ${memory} memory"
  # `container` 1.7.1 gives a node one CPU more than --cpus asks for.
  KUBECONFIG="${STATE}/kubeconfig" container cluster create --name "${CLUSTER}" --shared-image-store \
    --cpus "$((cpus - 1))" --memory "${memory}"
  kube apply -f /kind/manifests/default-storage.yaml
}

# The shipped seccomp profile on every node, checked byte for byte.
install_seccomp() {
  local want node
  want="$(shasum -a 256 "${REPO}/${SECCOMP}" | awk '{ print $1 }')"
  for node in $(kube get nodes -o name | sed 's|^node/||'); do
    container exec -i "${node}" sh -c \
      'mkdir -p /var/lib/kubelet/seccomp/profiles && cat > /var/lib/kubelet/seccomp/profiles/ultragraphdb.json' \
      <"${REPO}/${SECCOMP}"
    local have
    have="$(container exec "${node}" sha256sum /var/lib/kubelet/seccomp/profiles/ultragraphdb.json | awk '{ print $1 }')"
    [ "${have}" = "${want}" ] || fail "the seccomp profile on ${node} differs from ${SECCOMP}"
    echo "Seccomp profile on ${node}: ${have}"
  done
}

# Each node's containerd keeps the image it first resolved a name to and never reads the shared store's tag again, so its
# record of every image is dropped: the next pod resolves the name afresh, to the image just loaded.
forget_node_images() {
  local node entry
  for node in $(kube get nodes -o name </dev/null | sed 's|^node/||'); do
    for entry in "${IMAGES[@]}"; do
      container exec "${node}" crictl rmi "docker.io/${entry##* }" >/dev/null 2>&1 || true
    done
  done
}

# The chart's four Secrets, made only where absent: `helm template` reads no cluster, so each render draws new values, and a
# Secret already there keeps its own. The values pass from Helm to kubectl on a pipe and touch no file.
create_secrets() {
  local out
  out="$(render --show-only templates/secrets.yaml | kube -n "${NAMESPACE}" create -f - 2>&1)" || true
  if echo "${out}" | grep -i 'error' | grep -qv 'already exists'; then
    fail "making the Secrets: ${out}"
  fi
  echo "${out}" | { grep -v 'already exists' || true; }
}

wait_ready() {
  local workload
  for workload in "${WORKLOADS[@]}"; do
    kube -n "${NAMESPACE}" rollout status "${workload}" --timeout=300s
  done
}

# Each role's pod and the drivers its runtimes started on.
drivers() {
  local pod
  for pod in $(kube -n "${NAMESPACE}" get pods -l 'app.kubernetes.io/name in (config,frontend,storage,compute)' -o name); do
    local found
    found="$(kube -n "${NAMESPACE}" logs "${pod}" </dev/null | { grep -o 'a runtime on the [a-z_]* driver' || true; } | sort | uniq -c |
      awk '{ printf "%s%s x%s", sep, $6, $1; sep = ", " }')"
    echo "${pod#pod/}: ${found:-no runtime logged}"
  done
}

report() {
  container --version
  kube get nodes -o custom-columns='NODE:.metadata.name,CPUS:.status.capacity.cpu,MEMORY:.status.capacity.memory,KUBERNETES:.status.nodeInfo.kubeletVersion'
  kube -n "${NAMESPACE}" get pods -o wide
  drivers
}

up() {
  local cpus=12 memory=16g partitions=1
  while [ $# -gt 0 ]; do
    case "$1" in
      --cpus) cpus="$2"; shift 2 ;;
      --memory) memory="$2"; shift 2 ;;
      --partitions) partitions="$2"; shift 2 ;;
      *) fail "up takes --cpus, --memory and --partitions, not $1" ;;
    esac
  done
  build_and_load
  local existed=false
  if exists; then
    existed=true
    echo "The cluster ${CLUSTER} exists: deploying onto it"
  else
    create_cluster "${cpus}" "${memory}"
  fi
  install_seccomp
  forget_node_images
  node_sh "kubectl get namespace ${NAMESPACE} >/dev/null 2>&1 || kubectl create namespace ${NAMESPACE}" </dev/null
  create_secrets
  render --set secrets.create=false --set partitions="${partitions}" | kube apply -f -
  if [ "${existed}" = true ]; then
    # The images keep their names across a rebuild, so the pods start again to run the ones just loaded.
    kube -n "${NAMESPACE}" rollout restart "${WORKLOADS[@]}"
  fi
  wait_ready
  report
}

bench() {
  exists || fail "no cluster ${CLUSTER}: run make cluster-local first"
  local args=("$@")
  [ ${#args[@]} -gt 0 ] || args=("${DEFAULT_BENCH[@]}")
  local json="" arg
  for arg in "${args[@]}"; do
    case "${arg}" in
      *\"* | *\\*) fail "a bench argument holds a quote or a backslash: ${arg}" ;;
    esac
    json="${json}${json:+,}\"${arg}\""
  done
  kube -n "${NAMESPACE}" delete job bench --ignore-not-found --wait=true </dev/null
  render --set secrets.create=false --set bench.enabled=true --set-json "bench.args=[${json}]" \
    --show-only templates/bench-job.yaml | kube apply -f -
  echo "client_bench ${args[*]}"
  local state=""
  until [ -n "${state}" ]; do
    sleep 2
    state="$(kube -n "${NAMESPACE}" get job bench -o jsonpath='{.status.succeeded}{.status.failed}' </dev/null)"
  done
  kube -n "${NAMESPACE}" logs job/bench </dev/null
  [ "$(kube -n "${NAMESPACE}" get job bench -o jsonpath='{.status.succeeded}' </dev/null)" = 1 ] || fail "the bench Job failed"
}

status() {
  container cluster list
  exists || return 0
  report
}

down() {
  if exists; then
    KUBECONFIG="${STATE}/kubeconfig" container cluster delete --name "${CLUSTER}"
  fi
  echo "No cluster ${CLUSTER}"
}

needs container "Apple's container, 1.7 or later, from https://github.com/apple/container"
case "${1:-}" in
  up) shift; needs bazel "Bazel, through Bazelisk"; needs helm "Helm 3 or later, from https://helm.sh"; up "$@" ;;
  bench) shift; needs helm "Helm 3 or later, from https://helm.sh"; bench "$@" ;;
  status) status ;;
  down) down ;;
  *) fail "takes up, bench, status or down" ;;
esac
