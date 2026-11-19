#!/bin/bash
#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#
# One point of the cluster benchmark: a fresh two-role cluster per run on the machines of
# openspec/benches (ugdb-s storage, ugdb-f config service and compute, ugdb-c clients),
# a timed client_bench run, and what each machine's CPUs did meanwhile.
#
# usage: M7=<dir mounted at /m7> scripts/bench_cluster.sh <partitions> <op> <connections> [client_bench flags...]
#   RUNS="1 2 3"  the runs of the point (default three)
#   PIN=1         each role pins its runtimes and leaves CPU 0 to the machine's interrupts (default);
#                 PIN=0 runs the roles unpinned, as the record of 2026-11-09 did
#   KEEP=1        leaves the last cluster up
#
# $M7 holds bin/ (the four binaries, built -c opt for aarch64-unknown-linux-gnu) and keys/ (read.key,
# admin.key, role.secret, bootstrap.secret: 96 hexadecimal digits each, no newline).
set -u
: "${M7:?names the directory mounted at /m7 in each machine}"
P=$1; OP=$2; C=$3; shift 3
EXTRA="$*"
PIN=${PIN:-1}
OUT="$M7/run/results"
mkdir -p "$OUT" "$M7/run/logs" "$M7/profile"

address() { container list | awk -v m="$1" '$1 == m { sub("/.*", "", $6); print $6 }'; }
S=$(address ugdb-s)
F=$(address ugdb-f)

runtime() {
  [ "$PIN" = 1 ] && printf '[runtime]\npin_cores = true\npin_skip = 1\n'
}

settings() {
  cat > "$M7/profile/config.toml" <<EOF
[config]
address = "tcp://0.0.0.0:8090"
plaintext = true
data_dir = "/var/tmp/m7/config"
read_key_file = "/m7/keys/read.key"
admin_key_file = "/m7/keys/admin.key"
targets_file = "/m7/profile/targets.toml"
EOF
  cat > "$M7/profile/targets.toml" <<EOF
[split]
storage = "storage.toml"
compute = "compute.toml"
EOF
  # The storage instance hosts the system group, the metadata group and the sequencer, at an address of its own.
  cat > "$M7/profile/storage.toml" <<EOF
[cluster]
id = 1
role_secret_file = "/m7/keys/role.secret"
system_address = "tcp://$S:7081"
partition_count = $P
[observability]
address = "tcp://0.0.0.0:8083"
[storage]
partition_first = 1
partition_count = $P
role_address = "tcp://$S:7100"
data_dir = "/var/tmp/m7/storage"
[system]
role_address = "tcp://$S:7081"
data_dir = "/var/tmp/m7/system"
[metadata]
bootstrap_secret_file = "/m7/keys/bootstrap.secret"
[sequencer]
epoch_ms_min = 2
epoch_ms_max = 10
$(runtime)
EOF
  cat > "$M7/profile/compute.toml" <<EOF
[cluster]
id = 1
role_secret_file = "/m7/keys/role.secret"
system_address = "tcp://$S:7081"
[observability]
address = "tcp://0.0.0.0:8082"
[endpoint]
address = "tcp://0.0.0.0:7080"
plaintext = true
$(runtime)
EOF
}

down() {
  for m in ugdb-c ugdb-f ugdb-s; do
    container exec "$m" sh -c 'for p in /proc/[0-9]*; do n=$(cat $p/comm 2>/dev/null); case "$n" in ultragraphdb_*|client_bench) kill -9 ${p#/proc/} 2>/dev/null;; esac; done; true' > /dev/null
  done
  sleep 1
  for m in ugdb-f ugdb-s; do
    container exec "$m" sh -c 'rm -rf /var/tmp/m7/* && chown 65532:65532 /var/tmp/m7' > /dev/null
  done
}

role() { # machine instance binary
  container exec -d -u 65532:65532 \
    -e ULTRAGRAPHDB_CONFIG_URL="http://$F:8090" -e ULTRAGRAPHDB_CONFIG_TARGET=split \
    -e ULTRAGRAPHDB_CONFIG_INSTANCE="$2" -e ULTRAGRAPHDB_CONFIG_KEY_FILE=/m7/keys/read.key \
    "$1" sh -c "/m7/bin/$3 > /m7/run/logs/$2.log 2>&1" > /dev/null
}

up() {
  rm -f "$M7"/run/logs/*.log
  container exec -d -u 65532:65532 ugdb-f sh -c "/m7/bin/ultragraphdb_config --config /m7/profile/config.toml > /m7/run/logs/config.log 2>&1" > /dev/null
  sleep 1
  role ugdb-s storage ultragraphdb_storage
  role ugdb-f compute ultragraphdb_compute
  # Ready once a one-second client run succeeds.
  for _ in $(seq 1 60); do
    if container exec ugdb-c /m7/bin/client_bench --address "$F:7080" --password-file /m7/keys/bootstrap.secret \
         --op direct --connections 1 --partitions "$P" --seconds 1 > "$M7/run/logs/probe.log" 2>&1; then return 0; fi
    sleep 1
  done
  echo "cluster did not become ready" >&2; return 1
}

# A machine's CPU times, one line per CPU: busy outside interrupts, softirq, idle.
cpus() { container exec "$1" awk '/^cpu[0-9]/ { print $1, $2+$3+$4+$7, $8, $5+$6 }' /proc/stat; }

# What each CPU of the machine did between two samples, in percent: a CPU at 100% softirq is the machine's limit.
used() { # machine before after
  paste "$2" "$3" | awk -v m="$1" '{ b=$6-$2; s=$7-$3; i=$8-$4; t=b+s+i; if (t > 0) printf "%s %s busy=%.0f%% softirq=%.0f%% idle=%.0f%%\n", m, $1, 100*b/t, 100*s/t, 100*i/t }'
}

# One client process to 63 connections, two to 255, four from 256: a client machine of four CPUs.
procs=1; [ "$C" -ge 64 ] && procs=2; [ "$C" -ge 256 ] && procs=4
per=$((C / procs))
tag="p${P}_${OP}_c${C}_pin${PIN}$(echo "$EXTRA" | tr -d ' -')"
settings
for run in ${RUNS:-1 2 3}; do
  down
  if ! up; then echo "$tag run $run: NOT READY"; tail -3 "$M7"/run/logs/*.log; continue; fi
  for n in $(seq 1 $procs); do
    container exec ugdb-c /m7/bin/client_bench --address "$F:7080" --password-file /m7/keys/bootstrap.secret \
      --op "$OP" --connections "$per" --partitions "$P" --seconds 20 --seed "$n" $EXTRA \
      > "$OUT/${tag}_r${run}_n${n}.txt" 2>&1 &
  done
  # The timed part begins once every connection has written what it reads: sample the CPUs inside it.
  sleep 14
  for m in ugdb-c ugdb-f ugdb-s; do cpus $m > "$OUT/.cpu_a_$m"; done
  sleep 6
  for m in ugdb-c ugdb-f ugdb-s; do cpus $m > "$OUT/.cpu_b_$m"; done
  wait
  echo "== $tag run $run"
  cat "$OUT/${tag}_r${run}_n"*.txt
  for m in ugdb-c ugdb-f ugdb-s; do used $m "$OUT/.cpu_a_$m" "$OUT/.cpu_b_$m"; done | tee "$OUT/${tag}_r${run}_cpus.txt" | awk '$2 == "cpu0" || $4 == "softirq=100%"'
done
[ -n "${KEEP:-}" ] || down
