#!/usr/bin/env bash
#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#
# The lints the workspace's table holds back, denied for the crates named.
#
# ---------------------------------------------------------------------------------------------
# Usage
# ---------------------------------------------------------------------------------------------
#
#   scripts/held_back_lints.sh ultragraphdb_core_ids ultragraphdb_error_ids
#
# `[workspace.lints.clippy]` in the root `Cargo.toml` keeps four lints of Ultra Style's set
# commented out until the existing crates meet them. A crate created since meets them from its
# first commit; this runs Clippy over it with the four denied on the command line, so the one
# table stays as it is. `clippy.toml` allows indexing and slicing in tests.

set -o errexit
set -o nounset
set -o pipefail

if [ "$#" -lt 1 ]; then
    echo "usage: scripts/held_back_lints.sh <crate> [<crate> ...]" >&2
    exit 2
fi

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

PACKAGES=()
for crate in "$@"; do
    PACKAGES+=("-p" "$crate")
done

command cargo clippy "${PACKAGES[@]}" --all-targets -- \
    -D clippy::indexing_slicing \
    -D clippy::arithmetic_side_effects \
    -D clippy::cast_possible_truncation \
    -D clippy::cast_sign_loss
