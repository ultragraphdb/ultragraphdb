#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

# Runs every crate's BDD suite under Cargo and under Bazel and compares the scenarios each reports passed.
# A test cannot run both build systems, so this script does at the close of a stage.
set -o errexit
set -o nounset
set -o pipefail

strip() { sed 's/\x1b\[[0-9;]*m//g'; }
summary() { grep -Eo '[0-9]+ scenarios? \([^)]*\)' | tail -1; }

status=0
for dir in ultragraphdb_*/; do
  crate="${dir%/}"
  [ -d "$crate/features" ] || continue
  cargo_out=$(cargo test -p "$crate" --test bdd 2>&1 | strip | summary || true)
  bazel test "//$crate:bdd" >/dev/null 2>&1 || true
  bazel_out=$(strip < "target-bzl/testlogs/$crate/bdd/test.log" | summary || true)
  if [ -n "$cargo_out" ] && [ "$cargo_out" = "$bazel_out" ]; then
    echo "ok        $crate: $cargo_out"
  else
    echo "MISMATCH  $crate: cargo [$cargo_out] bazel [$bazel_out]"
    status=1
  fi
done
exit $status
