#!/usr/bin/env bash
#
# SPDX-License-Identifier: Apache-2.0
# Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
#
# Fuzzing: does a decoder survive the input nobody thought of?
#
# ---------------------------------------------------------------------------------------------
# What this catches that the suite and mutation testing do not
# ---------------------------------------------------------------------------------------------
#
# Mutation testing asks whether the suite notices a wrong line. It cannot find the input no test
# names, and that is where a decoder of bytes from a client, a peer or the disk fails: a length
# that overruns, an offset past the end, a checksum over the wrong range. `cargo fuzz` runs a
# crate's targets under libFuzzer, steering by coverage, and stops at the first input that panics
# or fails an assertion of the target.
#
# ---------------------------------------------------------------------------------------------
# Usage
# ---------------------------------------------------------------------------------------------
#
#   scripts/fuzz.sh                                     # every target of every crate with a fuzz package
#   scripts/fuzz.sh ultragraphdb_protocol_wire          # every target of one crate
#   scripts/fuzz.sh ultragraphdb_protocol_wire header   # one target
#
#   FUZZ_SECONDS=600 scripts/fuzz.sh ...                # time per target, 60 s by default
#   FUZZ_TOOLCHAIN=nightly-2026-08-08 scripts/fuzz.sh   # the nightly toolchain, `nightly` by default
#
# A crate is fuzzed when `fuzz/<crate>/` holds its cargo-fuzz package. The packages form one
# workspace in `fuzz/`, which the root workspace excludes and Bazel never builds, so they share one
# lock file and one build directory. Seeds for a target are read from the crate's
# `testdata/fuzz_seeds/<target>/` when it exists, the golden values the suite already holds.
#
# ---------------------------------------------------------------------------------------------
# When a target fails
# ---------------------------------------------------------------------------------------------
#
# libFuzzer writes the failing input to `fuzz/<crate>/artifacts/<target>/`. Minimize it with
# `cargo +nightly fuzz tmin --fuzz-dir fuzz/<crate> <target> <artifact>`, fix the decoder, and
# commit the minimized input under the crate's `testdata/fuzz_regressions/<target>/`, where a unit test
# replays it through `ultragraphdb_tool_bdd::fuzz_regressions` under Cargo and Bazel. Fuzzing is
# not hermetic and has no fixed end, so it stays outside Bazel, as mutation testing does.

set -o errexit
set -o nounset
set -o pipefail

TOOLCHAIN="${FUZZ_TOOLCHAIN:-nightly}"
SECONDS_PER_TARGET="${FUZZ_SECONDS:-60}"

if ! command -v cargo-fuzz >/dev/null 2>&1; then
    echo "cargo-fuzz is not installed. Install it with:" >&2
    echo "    cargo install cargo-fuzz --locked" >&2
    exit 1
fi
if ! cargo "+$TOOLCHAIN" --version >/dev/null 2>&1; then
    echo "fuzz.sh: the toolchain $TOOLCHAIN is not installed; cargo-fuzz needs a nightly. Install it with:" >&2
    echo "    rustup toolchain install $TOOLCHAIN" >&2
    exit 1
fi
case "$SECONDS_PER_TARGET" in
    '' | *[!0-9]*)
        echo "fuzz.sh: FUZZ_SECONDS must be a whole number of seconds, not '$SECONDS_PER_TARGET'" >&2
        exit 1
        ;;
esac

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

CRATE="${1:-}"
TARGET="${2:-}"

# A package name is not its directory, so ask Cargo, as mutants.sh does. A name Cargo does not know gives no line, not
# a failure: under pipefail grep's exit status would end the script before the caller can say why.
crate_dir() {
    { cargo metadata --no-deps --format-version 1 |
        tr ',' '\n' |
        grep -A0 "\"manifest_path\":[^,]*/$1/Cargo.toml" || true; } |
        sed -E 's|.*"manifest_path":"||; s|/Cargo.toml"?||' |
        sed "s|^$REPO_ROOT/||" |
        head -1
}

# Every crate with a package in the fuzz workspace, by name.
fuzzed_crates() {
    find fuzz -mindepth 2 -maxdepth 2 -type f -name Cargo.toml |
        sed -E 's|^fuzz/||; s|/Cargo.toml$||' |
        sort
}

FAILED=()
REPORT=()

# Runs one target of CRATE, whose sources are at DIR, for SECONDS_PER_TARGET and records its executions, coverage and
# outcome.
run_target() {
    local crate="$1" dir="$2" target="$3"
    local fuzz_dir="fuzz/$crate"
    local corpus="$fuzz_dir/corpus/$target"
    local seeds="$dir/testdata/fuzz_seeds/$target"
    local log
    mkdir -p "$corpus"
    log="$(mktemp)"
    local corpora=("$corpus")
    if [ -d "$seeds" ]; then
        corpora+=("$seeds")
    fi
    echo "== $crate: $target (${SECONDS_PER_TARGET}s)"
    if cargo "+$TOOLCHAIN" fuzz run --fuzz-dir "$fuzz_dir" "$target" "${corpora[@]}" -- \
        -max_total_time="$SECONDS_PER_TARGET" -print_final_stats=1 2>&1 | tee "$log" | grep -E "^(INFO|==|stat::)" ; then
        :
    fi
    local runs cov
    runs="$(grep -E '^stat::number_of_executed_units' "$log" | awk '{print $2}' | tail -1)"
    cov="$(grep -oE 'cov: [0-9]+' "$log" | tail -1 | awk '{print $2}')"
    if grep -qE 'SUMMARY: libFuzzer|panicked at|Test unit written to' "$log"; then
        FAILED+=("$crate:$target")
        REPORT+=("$crate $target: FAILED after ${runs:-?} executions, coverage ${cov:-?}; artifacts in $fuzz_dir/artifacts/$target/")
    else
        REPORT+=("$crate $target: ${runs:-?} executions, coverage ${cov:-?}, no failure")
    fi
    rm -f "$log"
}

# Runs every target of CRATE, or only TARGET when given.
run_crate() {
    local crate="$1" only="${2:-}"
    if [ ! -f "fuzz/$crate/Cargo.toml" ]; then
        echo "fuzz.sh: $crate has no package in fuzz/" >&2
        exit 1
    fi
    local dir
    dir="$(crate_dir "$crate")"
    if [ -z "$dir" ]; then
        echo "fuzz.sh: no workspace package named $crate" >&2
        exit 1
    fi
    local targets
    targets="$(cargo "+$TOOLCHAIN" fuzz list --fuzz-dir "fuzz/$crate")"
    if [ -n "$only" ]; then
        if ! printf '%s\n' "$targets" | grep -qx "$only"; then
            echo "fuzz.sh: $crate has no fuzz target named $only; it has:" >&2
            printf '    %s\n' $targets >&2
            exit 1
        fi
        targets="$only"
    fi
    local target
    for target in $targets; do
        run_target "$crate" "$dir" "$target"
    done
}

if [ -z "$CRATE" ]; then
    CRATES="$(fuzzed_crates)"
    if [ -z "$CRATES" ]; then
        echo "fuzz.sh: no crate has a package in fuzz/ yet"
        exit 0
    fi
    for crate in $CRATES; do
        run_crate "$crate"
    done
else
    run_crate "$CRATE" "$TARGET"
fi

echo
echo "Fuzzing report:"
printf '  %s\n' "${REPORT[@]}"
if [ "${#FAILED[@]}" -gt 0 ]; then
    echo "fuzz.sh: ${#FAILED[@]} target(s) failed; minimize each artifact with cargo fuzz tmin and commit it under" >&2
    echo "    <crate>/testdata/fuzz_regressions/<target>/ (see the header of this script)" >&2
    exit 2
fi
