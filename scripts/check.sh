#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

# bin/sh
set -o errexit
set -o nounset
set -o pipefail


# Check for outdated dependencies, the direct ones of each workspace member: cargo-outdated walks the full tree from every
# member, which on this workspace did not finish in 30 minutes (2026-11-17), while scripts/update.sh's cargo update keeps
# the transitive ones current.
# https://github.com/kbknapp/cargo-outdated
command cargo outdated --workspace --root-deps-only

# Scan for unused dependencies
command cargo machete-nk

# Check a package and all of its dependencies for errors.
command cargo check --all-targets

# Deny the wall clock, thread spawning and ambient randomness outside the uses that carry an #[allow] with its reason
# (clippy.toml, disallowed-methods), so that storage code stays deterministic under the simulator.
command cargo clippy --workspace --all-targets -- -D clippy::disallowed_methods

# Scan for security vulnerabilities
# https://crates.io/crates/cargo-audit
command cargo audit --deny warnings
