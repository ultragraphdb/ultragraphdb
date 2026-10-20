#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

# bin/sh
set -o errexit
set -o nounset
set -o pipefail


# Bazel file formatting (Installed via homebrew)
# https://github.com/bazelbuild/buildtools
buildifier -r MODULE.bazel BUILD.bazel
buildifier -r ultragraphdb_*/

# Rust code formatting
# https://github.com/rust-lang/rustfmt
command cargo fmt --all
