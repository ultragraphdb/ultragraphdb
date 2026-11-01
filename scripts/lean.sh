#!/usr/bin/env bash
#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#
# The Lean proofs: does every model in lean/ build, with no proof left open?
#
# ---------------------------------------------------------------------------------------------
# What this checks
# ---------------------------------------------------------------------------------------------
#
# `lean/` is the Lake project holding the repository's Lean proofs, the library `RoundModel` among them. `lake build`
# type-checks every proof, and a proof closed with `sorry` still builds: Lean reports it with the warning
# "declaration uses 'sorry'" and carries on. This script builds the project and fails on that warning, and on any
# `sorry` token in the sources, so a build that passes is a model without gaps. Lake replays the warnings of modules it
# has built before, and the source scan covers a module whose replayed log was lost.
#
# ---------------------------------------------------------------------------------------------
# Usage
# ---------------------------------------------------------------------------------------------
#
#   scripts/lean.sh
#
# The toolchain is the one `lean/lean-toolchain` pins; elan installs it on first use. Lean is not hermetic under Bazel
# and has no Bazel rules here, so the proofs build outside it, as fuzzing and mutation testing run.

set -o errexit
set -o nounset
set -o pipefail

if ! command -v lake >/dev/null 2>&1; then
    echo "lake is not installed. Install elan, which installs the pinned toolchain on first use:" >&2
    echo "    curl https://raw.githubusercontent.com/leanprover/elan/master/elan-init.sh -sSf | sh" >&2
    exit 1
fi

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT/lean"

log="$(mktemp)"
trap 'rm -f "$log"' EXIT

echo "== lake build in lean/ ($(cat lean-toolchain))"
if ! lake build 2>&1 | tee "$log"; then
    echo "lean.sh: lake build failed" >&2
    exit 1
fi

status=0
if grep -q "declaration uses 'sorry'" "$log"; then
    echo "lean.sh: a proof uses sorry:" >&2
    grep "declaration uses 'sorry'" "$log" >&2
    status=1
fi
# Every Lean source of the project, the Lake build output excluded.
if find . -path ./.lake -prune -o -type f -name '*.lean' -print0 | xargs -0 grep -nw 'sorry' >&2; then
    echo "lean.sh: the sources above contain sorry" >&2
    status=1
fi
if [ "$status" -eq 0 ]; then
    echo "lean.sh: lake build passed; no proof uses sorry"
fi
exit "$status"
