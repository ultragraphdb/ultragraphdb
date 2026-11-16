#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#
# bin/sh
#
# Runs every property test with fresh random seeds, outside Bazel's cache.
#
# `make test` runs each property target derandomized (HEGEL_DERANDOMIZE=true in its BUILD `env`), so a run is a function
# of its sources and Bazel caches it; it therefore tries one seed, always the same. A counterexample another seed would
# draw is never found there. This script builds every target tagged `property-test` and runs its binary directly,
# PROPERTY_RUNS times (20 unless set), each with a fresh seed and with shrinking on, so a failure prints its minimal
# counterexample and the line that reproduces it. It ends with a summary and fails when any run failed.
#
# Run it before a task group closes and at a stage's final verification:
#   make property-random                     # every property target, 20 runs each
#   PROPERTY_RUNS=50 make property-random    # more runs
set -o errexit
set -o nounset
set -o pipefail

RUNS=${PROPERTY_RUNS:-20}
LOGS=$(mktemp -d)
mapfile -t TARGETS < <(command bazel query 'attr(tags, "property-test", tests(//...))' 2>/dev/null)
echo "=============="
echo "Build ${#TARGETS[@]} property targets"
echo "=============="
command bazel build "${TARGETS[@]}"

FAILED=()
for target in "${TARGETS[@]}"; do
  exe=$(command bazel cquery --output=files "$target" 2>/dev/null | head -n 1)
  exe="$(command bazel info execution_root 2>/dev/null)/$exe"
  passed=0
  for run in $(seq 1 "$RUNS"); do
    log="$LOGS/$(echo "$target" | tr '/:' '__')_$run.log"
    # The variables a test reads under Bazel, so it finds its package's files in the runfiles tree.
    if (cd "$exe.runfiles/_main" && env TEST_SRCDIR="$exe.runfiles" TEST_WORKSPACE=_main TEST_TARGET="$target" \
        HEGEL_DERANDOMIZE=false HEGEL_DATABASE=disabled \
        HEGEL_PHASES=explicit,reuse,generate,target,shrink "$exe") > "$log" 2>&1; then
      passed=$((passed + 1))
    else
      FAILED+=("$target run $run: $log")
    fi
  done
  echo "$target: $passed of $RUNS runs passed"
done

echo ""
if [ "${#FAILED[@]}" -eq 0 ]; then
  echo "Every property held under $RUNS fresh seeds per target."
  rm -rf "$LOGS"
else
  echo "Failed runs; each log holds the counterexample and its reproduce line:"
  printf '  %s\n' "${FAILED[@]}"
  exit 1
fi
