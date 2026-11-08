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
            w.checked(),
            w.skipped(),
            w.unrecovered(),
            w.writes()
        );
        // The check reads with no read failing, so only a step left needing recovery goes unchecked, and a check
        // is skipped only when the applier's reopening before it fails. Observed over seeds 100..=111 at 300 steps:
        // 301 to 307 steps checked per seed, none skipped. A run skipping checks fails here.
        assert_eq!(
            w.skipped(),
            0,
            "seed {seed}: {} checks skipped",
            w.skipped()
        );
        assert!(
            w.checked() >= 300,
            "seed {seed}: only {} steps checked",
            w.checked()
        );
        checked += w.checked();
        writes += w.writes();
    }
    assert!(checked >= 3600, "only {checked} checks");
    assert!(writes > 1000, "only {writes} writes");
    Ok(())
}
