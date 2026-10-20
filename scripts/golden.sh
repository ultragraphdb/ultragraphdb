#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

# Regenerates golden vectors from Dgraph's Go source (the fork beside this repository, or $DGRAPH_SRC).
# Needs a Go toolchain; building and testing ultragraphdb does not.
set -o errexit
set -o nounset
set -o pipefail

# Go on arm64 fuses multiply-adds, which changes the last bit of S2 vertices; with fusing off, Go computes what it
# computes on amd64, which is also what Rust computes, as Rust never fuses without an explicit mul_add.
export GOFLAGS="${GOFLAGS:-} -gcflags=all=-d=fmahash=n"

cd "$(dirname "$0")/../tools/golden"
go run . "$@"
