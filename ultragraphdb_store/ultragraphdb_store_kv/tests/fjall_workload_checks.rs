/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The simulator's store workload run on fjall beside the simulated store, without faults: both answer every
//! operation alike and hold the same versions after every step. A target of its own, so Bazel runs it beside the
//! crash replay.

use ultragraphdb_store_sim::FaultConfig;
use ultragraphdb_tool_test::{Run, run_both};

#[test]
fn test_the_store_workload_passes_the_same_checks_on_fjall() {
    let runs: Vec<Run> = (1..=12)
        .map(|seed| run_both(seed, FaultConfig::none(), 300, 2).unwrap_or_else(|e| panic!("{e}")))
        .collect();
    let replayed: usize = runs.iter().map(|r| r.replayed()).sum();
    let fenced: usize = runs.iter().map(|r| r.fenced()).sum();
    let (reads, found) = runs.iter().fold((0, 0), |(r, f), run| {
        (r + run.versionless_reads().0, f + run.versionless_reads().1)
    });
    // Evidence that the run did something: a skip and a pass look alike.
    println!(
        "replayed {replayed} operations on fjall over 12 seeds; {fenced} fenced entries written by prefix drops; \
         {reads} versionless reads, {found} of them finding a value"
    );
    assert!(replayed > 12 * 300);
    assert!(
        found > 0 && found < reads,
        "versionless reads found {found} values in {reads} reads"
    );
}
