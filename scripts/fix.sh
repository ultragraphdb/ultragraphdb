#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#

# bin/sh
set -o errexit
set -o nounset
set -o pipefail


# https://users.rust-lang.org/t/ive-ran-clippy-on-100k-crates/117127
command cargo clippy --workspace --all-targets --fix --allow-dirty -- -Wclippy::all
