#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

# bin/sh
set -o errexit
set -o nounset
set -o pipefail


echo "=============="
echo "Check retired names"
echo "=============="

# role-binaries, "Nothing is named frontend": the workspace's members, the images (`bazel/`), the chart (`deploy/`) and
# the profiles name `frontend` only where `scripts/frontend_leftovers.txt` lists the file and its count of lines.
ROOT=$(git rev-parse --show-toplevel)
SCOPE=('ultragraphdb*' bazel deploy)
FOUND=$(git -C "$ROOT" grep --untracked -ic frontend -- "${SCOPE[@]}" | sort || true)
ALLOWED=$(grep -v -e '^#' -e '^$' "$ROOT/scripts/frontend_leftovers.txt" | awk '{print $1}' | sort)
NAMED=$( (git -C "$ROOT" ls-files -- "${SCOPE[@]}"; git -C "$ROOT" ls-files --others --exclude-standard -- "${SCOPE[@]}") | grep -i frontend || true)
if [ "$FOUND" != "$ALLOWED" ] || [ -n "$NAMED" ]; then
  if [ "$FOUND" != "$ALLOWED" ]; then
    echo "Lines naming frontend differ from scripts/frontend_leftovers.txt (< found, > listed):"
    diff <(echo "$FOUND") <(echo "$ALLOWED") || true
  fi
  if [ -n "$NAMED" ]; then
    echo "Files named frontend:"
    echo "$NAMED"
  fi
  exit 1
fi

echo ""
echo "=============="
echo "Build targets "
echo "=============="

command bazel build //...

echo ""
echo "=============="
echo "Run doc tests"
echo "=============="

command bazel test //... --test_tag_filters=doc-test

echo ""
echo "=============="
echo "Run unit tests"
echo "=============="

command bazel test //... --test_tag_filters=unit-test

echo ""
echo "=============="
echo "Run property tests"
echo "=============="

command bazel test //... --test_tag_filters=property-test

echo ""
echo "=============="
echo "Run concurrency tests"
echo "=============="

command bazel test //... --test_tag_filters=loom-test

echo ""
echo "=============="
echo "Run BDD tests"
echo "=============="

command bazel test //... --test_tag_filters=bdd-test

echo ""
echo "=============="
echo "Run integration tests"
echo "=============="

# One group per binary under test, the groups in sequence and each group's targets side by side, so the machine runs one
# binary's processes at a time. Each target listens in a block of ports of its own (`bazel/test_ports.bzl`); the image
# tests, which run Docker containers, are `exclusive`.
INTEGRATION_GROUPS=(config storage compute supervisor cluster)

# Every integration target belongs to one group, or the sequence below would skip it without a word.
UNGROUPED=$(command bazel query 'attr(tags, "integration_test", tests(//...)) except attr(tags, "-integration-test", tests(//...))' 2>/dev/null)
if [ -n "$UNGROUPED" ]; then
  echo "Integration targets without a group tag (<group>-integration-test):"
  echo "$UNGROUPED"
  exit 1
fi

for group in "${INTEGRATION_GROUPS[@]}"; do
  echo ""
  echo "Integration tests: $group"
  command bazel test //... --test_tag_filters="$group-integration-test"
done

echo ""
echo "====================="
echo "All Tests Passed"
echo "====================="
echo ""
echo "* ✅ Passed: Retired Names"
echo "* ✅ Passed: Doc Tests"
echo "* ✅ Passed: Unit Tests"
echo "* ✅ Passed: Property Tests"
echo "* ✅ Passed: Concurrency Tests"
echo "* ✅ Passed: BDD Tests"
echo "* ✅ Passed: Integration Tests"
echo ""
