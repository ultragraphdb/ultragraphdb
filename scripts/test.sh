#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

# bin/sh
set -o errexit
set -o nounset
set -o pipefail


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
INTEGRATION_GROUPS=(config storage compute frontend supervisor cluster)

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
echo "* ✅ Passed: Doc Tests"
echo "* ✅ Passed: Unit Tests"
echo "* ✅ Passed: Property Tests"
echo "* ✅ Passed: Concurrency Tests"
echo "* ✅ Passed: BDD Tests"
echo "* ✅ Passed: Integration Tests"
echo ""
