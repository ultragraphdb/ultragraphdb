#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

# bin/sh
set -o errexit
set -o nounset
set -o pipefail


# Check for outdated dependencies
# https://github.com/kbknapp/cargo-outdated
command cargo outdated --workspace

# Scan for unused dependencies
command cargo machete-nk

# Check a package and all of its dependencies for errors.
command cargo check --all-targets

# Scan for security vulnerabilities
# https://crates.io/crates/cargo-audit
command cargo audit --deny warnings
