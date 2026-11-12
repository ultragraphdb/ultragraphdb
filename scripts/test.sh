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

# Each target is `exclusive`, since the binaries listen at declared ports.
command bazel test //... --test_tag_filters=integration_test

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
