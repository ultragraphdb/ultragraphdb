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
echo "====================="
echo "All Tests Passed"
echo "====================="
echo ""
echo "* ✅ Passed: doc tests"
echo "* ✅ Passed: unit tests"
echo ""
