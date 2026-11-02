#!/usr/bin/env bash
#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#
# Coverage: does every line a change adds run under its crate's suite?
#
# ---------------------------------------------------------------------------------------------
# Usage
# ---------------------------------------------------------------------------------------------
#
#   scripts/coverage.sh ultragraphdb_core_ids                     # every file of the crate
#   scripts/coverage.sh ultragraphdb_core_ids src/types/bucket.rs # the files named, relative to the crate
#
# `cargo llvm-cov -p <crate> --lcov` runs the crate's whole suite, unit and BDD targets alike, and
# writes `target/coverage/<crate>.lcov`. The script then lists each uncovered line of the files
# named, or of every file under the crate's `src/` when none is named, and exits 1 when any is
# left: `tdd-protocol.md`'s phase 4 asks every added line covered, and a line left uncovered is
# covered by a test or listed with its reason in the group's notes.
#
# Bazel is the test gate; this is the one measurement that runs a suite under Cargo, so a crate
# measured here passes under `cargo test -p` as well.

set -o errexit
set -o nounset
set -o pipefail

if ! command -v cargo-llvm-cov >/dev/null 2>&1; then
    echo "cargo-llvm-cov is not installed. Install it with:" >&2
    echo "    cargo install cargo-llvm-cov --locked" >&2
    exit 1
fi

if [ "$#" -lt 1 ]; then
    echo "usage: scripts/coverage.sh <crate> [<file relative to the crate> ...]" >&2
    exit 2
fi

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

CRATE="$1"
shift

# A package name is not its directory, so ask Cargo for the manifest's directory.
CRATE_DIR="$(cargo metadata --format-version 1 --no-deps \
    | python3 -I -c 'import json, sys, os
name = sys.argv[1]
for p in json.load(sys.stdin)["packages"]:
    if p["name"] == name:
        print(os.path.dirname(p["manifest_path"]))
        break' "$CRATE")"
if [ -z "$CRATE_DIR" ]; then
    echo "no workspace package is named $CRATE" >&2
    exit 2
fi

OUT_DIR="$REPO_ROOT/target/coverage"
LCOV="$OUT_DIR/$CRATE.lcov"
mkdir -p "$OUT_DIR"

cargo llvm-cov -p "$CRATE" --all-targets --lcov --output-path "$LCOV"

if [ "$#" -gt 0 ]; then
    FILES=()
    for file in "$@"; do
        FILES+=("$CRATE_DIR/$file")
    done
else
    FILES=("$CRATE_DIR/src/")
fi

# lcov: `SF:<path>` opens a file's record, `DA:<line>,<count>` gives a line's count. A record
# counts when its path is one of the files named or lies under a directory named.
python3 -I - "$LCOV" "${FILES[@]}" <<'EOF'
import sys

lcov, wanted = sys.argv[1], sys.argv[2:]

def selected(path):
    return any(path == w or (w.endswith("/") and path.startswith(w)) for w in wanted)

uncovered, current = [], None
with open(lcov, encoding="utf-8") as records:
    for line in records:
        line = line.rstrip("\n")
        if line.startswith("SF:"):
            current = line[3:] if selected(line[3:]) else None
        elif line.startswith("DA:") and current is not None:
            number, count = line[3:].split(",")[:2]
            if int(count) == 0:
                uncovered.append(f"{current}:{number}")

for entry in uncovered:
    print(f"uncovered: {entry}")
print(f"{len(uncovered)} uncovered lines")
sys.exit(1 if uncovered else 0)
EOF
