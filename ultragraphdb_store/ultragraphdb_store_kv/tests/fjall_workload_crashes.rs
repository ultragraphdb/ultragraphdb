/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The simulator's store workload run on fjall beside the simulated store, with a crash between steps replayed as a
//! close and a reopen of fjall. A target of its own, so Bazel runs it beside the fault-free run.

use ultragraphdb_store_sim::FaultConfig;
use ultragraphdb_tool_test::{Run, run_both};

#[test]
fn test_the_store_workload_survives_crashes_on_fjall() {
    let runs: Vec<Run> = (101..=106)
        .map(|seed| {
            run_both(
                seed,
                FaultConfig::none().with_crash_between_steps(30),
                300,
                2,
            )
            .unwrap_or_else(|e| panic!("{e}"))
        })
        .collect();
    let recoveries: usize = runs.iter().map(|r| r.recoveries()).sum();
    println!("reopened fjall {recoveries} times over 6 seeds");
    assert!(recoveries >= 6);
}
