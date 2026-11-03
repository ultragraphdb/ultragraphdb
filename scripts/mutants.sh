#!/usr/bin/env bash
#
# SPDX-License-Identifier: MIT
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#
# Mutation testing: does the suite notice when the source is wrong?
#
# ---------------------------------------------------------------------------------------------
# What this catches that coverage does not
# ---------------------------------------------------------------------------------------------
#
# Coverage reports whether a line ran. A line can run and still be untested: assert a quantity
# that is zero for every input under test and any implementation of it passes. That is how the
# topological-charge normalization in `deep_causality_topology` stayed eight times too small
# through 513 executions of the line that computed it.
#
# `cargo mutants` changes one expression at a time and re-runs the tests. A mutant the tests do
# not object to is a decision nothing pins.
#
# ---------------------------------------------------------------------------------------------
# Usage
# ---------------------------------------------------------------------------------------------
#
#   scripts/mutants.sh <crate> <file> --diff <base>       # the lines of <file> changed since <base>
#   scripts/mutants.sh <crate> <file> --diff <base> --re <regex>   # only the mutants <regex> names
#   scripts/mutants.sh <crate> <file>                     # every mutant of <file>
#
# <file> is relative to the crate's directory; <base> is a git revision, the commit a task group
# started from. `--re` re-runs the mutants a fix pass killed, and nothing else.
#
# Equivalent mutants are excluded in //.cargo/mutants.toml, each with its reason. A survivor not
# listed there is a real gap.
#
# ---------------------------------------------------------------------------------------------
# The budget
# ---------------------------------------------------------------------------------------------
#
# Every mutant costs a build plus a full test run for its crate, so a run is bounded three ways:
#
# - At most three runs per file and base. A ledger under target/mutants-ledger/ counts them; the
#   fourth run is refused, and what still survives is recorded for the stage's final verification.
#   The intended sequence is one run over the diff, one fix pass, one re-run of the killed mutants.
# - One run at a time on this machine: a run takes target/mutants.lock (shlock, stale locks freed
#   by PID) and waits up to MUTANTS_LOCK_WAIT seconds (3,600 by default) for it.
# - MUTANTS_JOBS parallel jobs (4 by default).
#
# `--diff` mutates only the lines a group changed, through cargo-mutants' `--in-diff`, so a group
# tests its own code rather than every expression of an older file.

set -o errexit
set -o nounset
set -o pipefail

if ! command -v cargo-mutants >/dev/null 2>&1; then
    echo "cargo-mutants is not installed. Install it with:" >&2
    echo "    cargo install cargo-mutants --locked" >&2
    exit 1
fi

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

usage() {
    echo "usage: scripts/mutants.sh <crate> <file> [--diff <base>] [--re <regex>]" >&2
    exit 2
}

[ "$#" -ge 2 ] || usage
CRATE="$1"
FILE="$2"
shift 2
BASE=""
RE=""
while [ "$#" -gt 0 ]; do
    case "$1" in
        --diff) [ "$#" -ge 2 ] || usage; BASE="$2"; shift 2 ;;
        --re) [ "$#" -ge 2 ] || usage; RE="$2"; shift 2 ;;
        *) usage ;;
    esac
done

JOBS="${MUTANTS_JOBS:-4}"
LOCK_WAIT="${MUTANTS_LOCK_WAIT:-3600}"
MAX_RUNS=3

# A package name is not its directory, so `--file` needs the manifest's directory.
crate_dir() {
    cargo metadata --no-deps --format-version 1 |
        tr ',' '\n' |
        grep -A0 "\"manifest_path\":[^,]*/$1/Cargo.toml" |
        sed -E 's|.*"manifest_path":"||; s|/Cargo.toml"?||' |
        sed "s|^$REPO_ROOT/||" |
        head -1
}

DIR="$(crate_dir "$CRATE")"
if [ -z "$DIR" ]; then
    echo "mutants.sh: no workspace package named $CRATE" >&2
    exit 1
fi
PATH_IN_REPO="$DIR/$FILE"
if [ ! -f "$PATH_IN_REPO" ]; then
    echo "mutants.sh: no file $PATH_IN_REPO" >&2
    exit 1
fi

# The diff a run mutates, settled before the ledger is touched: an unchanged file costs no run.
DIFF_FILE=""
if [ -n "$BASE" ]; then
    DIFF_FILE="$(mktemp)"
    trap 'rm -f "$DIFF_FILE"' EXIT
    git diff "$BASE" -- "$PATH_IN_REPO" > "$DIFF_FILE"
    if [ ! -s "$DIFF_FILE" ]; then
        # An untracked file has no diff against the base: it is new, so all of it is the change.
        if git ls-files --error-unmatch "$PATH_IN_REPO" >/dev/null 2>&1; then
            rm -f "$DIFF_FILE"
            echo "mutants.sh: $PATH_IN_REPO is unchanged since $BASE; nothing to mutate"
            exit 0
        fi
        git diff --no-index /dev/null "$PATH_IN_REPO" > "$DIFF_FILE" || true
    fi
fi

# The ledger: one count per file and base.
LEDGER_DIR="$REPO_ROOT/target/mutants-ledger"
mkdir -p "$LEDGER_DIR"
LEDGER="$LEDGER_DIR/$(printf '%s@%s' "$PATH_IN_REPO" "${BASE:-full}" | tr '/' '_')"
RUNS="$(cat "$LEDGER" 2>/dev/null || echo 0)"
if [ "$RUNS" -ge "$MAX_RUNS" ]; then
    echo "mutants.sh: $PATH_IN_REPO has had its $MAX_RUNS runs for base ${BASE:-full}." >&2
    echo "Record what still survives in the group's notes for the stage's final verification." >&2
    exit 3
fi

# One run at a time on this machine.
LOCK="$REPO_ROOT/target/mutants.lock"
waited=0
until shlock -p $$ -f "$LOCK"; do
    if [ "$waited" -ge "$LOCK_WAIT" ]; then
        echo "mutants.sh: another mutants run held $LOCK for ${LOCK_WAIT}s; try again later" >&2
        exit 4
    fi
    sleep 10
    waited=$((waited + 10))
done
cleanup() {
    rm -f "$LOCK"
    if [ -n "$DIFF_FILE" ]; then rm -f "$DIFF_FILE"; fi
}
trap cleanup EXIT

echo $((RUNS + 1)) > "$LEDGER"
echo "mutants.sh: run $((RUNS + 1)) of $MAX_RUNS for $PATH_IN_REPO (base ${BASE:-full}, $JOBS jobs)"

ARGS=(-p "$CRATE" --file "$PATH_IN_REPO" -j "$JOBS")
if [ -n "$DIFF_FILE" ]; then
    ARGS+=(--in-diff "$DIFF_FILE")
fi
if [ -n "$RE" ]; then
    ARGS+=(--re "$RE")
fi

cargo mutants "${ARGS[@]}"
