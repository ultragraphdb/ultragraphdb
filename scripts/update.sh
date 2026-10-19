#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

# bin/sh
set -o errexit
set -o nounset
set -o pipefail


command echo ""
command echo "Checking for rustc stable update"
command rustup update stable
command echo ""
command echo "Checking for cargo updates"
command cargo update
