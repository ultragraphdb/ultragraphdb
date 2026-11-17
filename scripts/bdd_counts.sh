#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

# Runs every crate's BDD suite under Cargo and under Bazel and compares the scenarios each reports passed.
# A test cannot run both build systems, so this script does at the close of a stage.
# A crate's suite is every Cargo test it declares with `harness = false`, and under Bazel every test target named `bdd`,
# `bdd_*` or `*_bdd` in its package, since a slow suite is split over several targets. Each side sums the summaries its
# logs hold, one per run of a binary.
set -o errexit
set -o nounset
set -o pipefail

strip() { sed 's/\x1b\[[0-9;]*m//g'; }
total() {
  grep -Eo '[0-9]+ scenarios? \([^)]*\)' | awk '
    { scenarios += $1; if (match($0, /[0-9]+ passed/)) passed += substr($0, RSTART, RLENGTH) + 0 }
    END { if (NR > 0) printf "%d scenarios (%d passed)\n", scenarios, passed }'
}
# The tests of a manifest that run with the libtest harness off.
cargo_suites() {
  awk '/^\[/ { if (test && off) print name; test = ($0 == "[[test]]"); name = ""; off = 0; next }
    test && /^name *=/ { name = $0; sub(/^name *= *"/, "", name); sub(/".*/, "", name) }
    test && /^harness *= *false/ { off = 1 }
    END { if (test && off) print name }' "$1"
}

targets=$(bazel query 'attr(name, "^(bdd|bdd_.+|.+_bdd)$", tests(//...))' 2>/dev/null)
bazel test --keep_going $targets >/dev/null 2>&1 || true

status=0
for dir in ultragraphdb_*/ ultragraphdb_*/ultragraphdb_*/; do
  package="${dir%/}"
  crate="${package##*/}"
  [ -f "$package/Cargo.toml" ] && [ -d "$package/features" ] || continue
  suites=$(cargo_suites "$package/Cargo.toml" | sed 's/^/--test /' | tr '\n' ' ')
  cargo_out=""
  if [ -n "$suites" ]; then
    cargo_out=$(cargo test --no-fail-fast -p "$crate" $suites 2>&1 | strip | total || true)
  fi
  bazel_out=$(echo "$targets" | grep "^//$package:" | while read -r target; do
    log="target-bzl/testlogs/$package/${target##*:}/test.log"
    if [ -f "$log" ]; then strip < "$log"; fi
  done | total || true)
  if [ -n "$cargo_out" ] && [ "$cargo_out" = "$bazel_out" ]; then
    echo "ok        $crate: $cargo_out"
  else
    echo "MISMATCH  $crate: cargo [$cargo_out] bazel [$bazel_out]"
    status=1
  fi
done
exit $status
