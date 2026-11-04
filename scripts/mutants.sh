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
#   scripts/mutants.sh <crate> --diff <base>                  # every line of <crate> changed since <base>
#   scripts/mutants.sh <crate> --diff <base> --re <regex>     # only the mutants <regex> names
#
# One run covers the whole crate: every file it changed since <base>, the commit its task group started from,
# untracked files included. A crate pays its build and test baseline once, and its mutants spread over the jobs,
# rather than one baseline per file queued behind the lock. `--re` re-runs the mutants a fix pass killed, and nothing
# else. A file argument is refused.
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
# - At most three runs per crate and base. A ledger under target/mutants-ledger/ counts them; the
#   fourth run is refused, and what still survives is recorded for the stage's final verification.
#   The intended sequence is one run over the diff, one fix pass, one re-run of the killed mutants.
# - One run at a time on this machine: a run takes target/mutants.lock (shlock, stale locks freed
#   by PID) and waits up to MUTANTS_LOCK_WAIT seconds (3,600 by default) for it. MUTANTS_SLOT names a
#   second lane, with a lock and an output directory of its own (target/mutants-slot-<name>/mutants.out),
#   for a second crate beside the first; give it fewer jobs through MUTANTS_JOBS.
# - MUTANTS_JOBS parallel jobs (8 by default: one run holds the machine at a time, so it takes 8 of its 12
#   performance cores and leaves the rest to the agents' builds).

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
    echo "usage: scripts/mutants.sh <crate> --diff <base> [--re <regex>]" >&2
    echo "One run covers every file the crate changed since <base>; a file argument is refused." >&2
    exit 2
}

[ "$#" -ge 3 ] || usage
CRATE="$1"
shift
BASE=""
RE=""
while [ "$#" -gt 0 ]; do
    case "$1" in
        --diff) [ "$#" -ge 2 ] || usage; BASE="$2"; shift 2 ;;
        --re) [ "$#" -ge 2 ] || usage; RE="$2"; shift 2 ;;
        *) usage ;;
    esac
done
[ -n "$BASE" ] || usage

JOBS="${MUTANTS_JOBS:-8}"
LOCK_WAIT="${MUTANTS_LOCK_WAIT:-3600}"
MAX_RUNS=3

# A package name is not its directory, so the diff needs the manifest's directory.
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

# The diff a run mutates, settled before the ledger is touched: an unchanged crate costs no run. Untracked files under
# src/ are new, so all of each is the change.
DIFF_FILE="$(mktemp)"
SLOT="${MUTANTS_SLOT:-}"
LOCK="$REPO_ROOT/target/mutants${SLOT:+.$SLOT}.lock"
HOLDS_LOCK=0
cleanup() {
    rm -f "$DIFF_FILE"
    if [ "$HOLDS_LOCK" -eq 1 ]; then rm -f "$LOCK"; fi
}
trap cleanup EXIT
git diff "$BASE" -- "$DIR/src" > "$DIFF_FILE"
while IFS= read -r untracked; do
    git diff --no-index /dev/null "$untracked" >> "$DIFF_FILE" || true
done < <(git ls-files --others --exclude-standard -- "$DIR/src")
if [ ! -s "$DIFF_FILE" ]; then
    echo "mutants.sh: $DIR/src is unchanged since $BASE; nothing to mutate"
    exit 0
fi

# The ledger: one count per crate and base.
LEDGER_DIR="$REPO_ROOT/target/mutants-ledger"
mkdir -p "$LEDGER_DIR"
LEDGER="$LEDGER_DIR/$(printf '%s@%s' "$CRATE" "$BASE" | tr '/' '_')"
RUNS="$(cat "$LEDGER" 2>/dev/null || echo 0)"
if [ "$RUNS" -ge "$MAX_RUNS" ]; then
    echo "mutants.sh: $CRATE has had its $MAX_RUNS runs for base $BASE." >&2
    echo "Record what still survives in the group's notes for the stage's final verification." >&2
    exit 3
fi

# One run at a time on this machine.
waited=0
until shlock -p $$ -f "$LOCK"; do
    if [ "$waited" -ge "$LOCK_WAIT" ]; then
        echo "mutants.sh: another mutants run held $LOCK for ${LOCK_WAIT}s; try again later" >&2
        exit 4
    fi
    sleep 10
    waited=$((waited + 10))
done
HOLDS_LOCK=1

echo $((RUNS + 1)) > "$LEDGER"
echo "mutants.sh: run $((RUNS + 1)) of $MAX_RUNS for $CRATE (base $BASE, $JOBS jobs)"

ARGS=(-p "$CRATE" --in-diff "$DIFF_FILE" -j "$JOBS")
if [ -n "$SLOT" ]; then
    mkdir -p "$REPO_ROOT/target/mutants-slot-$SLOT"
    ARGS+=(--output "$REPO_ROOT/target/mutants-slot-$SLOT")
fi
if [ -n "$RE" ]; then
    ARGS+=(--re "$RE")
fi

cargo mutants "${ARGS[@]}"
