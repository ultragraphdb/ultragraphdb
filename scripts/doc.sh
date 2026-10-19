#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

# bin/sh
set -o errexit
set -o nounset
set -o pipefail


command bazel build //... --build_tag_filters=doc,doc-test
command bazel test  //... --test_tag_filters=doc-test
