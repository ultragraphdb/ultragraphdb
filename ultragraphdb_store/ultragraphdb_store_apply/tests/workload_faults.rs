/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The changeset workload's fixed seeds with every fault class: every index agrees with the data after every step and
//! every recovery.

use ultragraphdb_store_sim::Seed;
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_tool_test::ChangesetWorkload;

#[test]
fn test_seeds_with_faults_keep_every_index() -> TestResult {
    let (mut checked, mut writes) = (0, 0);
    for seed in 100..=111 {
        let (_, w) = ChangesetWorkload::run(Seed::new(seed), ChangesetWorkload::faults(), 300)
            .map_err(|v| v.to_string())
            .context("the workload breaks no invariant")?;
        println!(
            "seed {seed}: {} checked, {} skipped, {} unrecovered, {} writes",
            w.checked, w.skipped, w.unrecovered, w.writes
        );
        // A check whose reads an injected fault fails on every retry is skipped, as is a step left needing
        // recovery. Observed over seeds 100..=111 at 300 steps: 118 to 193 steps checked per seed, and checked at
        // least 38% of the steps whose check ran or was skipped. A run skipping most checks fails here.
        assert!(
            w.checked >= 100,
            "seed {seed}: only {} steps checked",
            w.checked
        );
        assert!(
            w.checked * 3 >= w.checked + w.skipped,
            "seed {seed}: {} checked, {} skipped: under a third checked",
            w.checked,
            w.skipped
        );
        checked += w.checked;
        writes += w.writes;
    }
    assert!(checked > 1000, "only {checked} checks");
    assert!(writes > 1000, "only {writes} writes");
    Ok(())
}
