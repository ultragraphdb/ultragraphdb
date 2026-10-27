/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The changeset workload's fixed seeds without faults: the store compared with the model after every step. The
//! seeds with faults and a seed's replay are targets of their own, so Bazel runs the three in parallel.

use ultragraphdb_store_sim::{FaultConfig, Seed};
use ultragraphdb_tool_test::ChangesetWorkload;

#[test]
fn test_seeds_without_faults_agree_with_the_model() {
    let (mut writes, mut refusals, mut compared) = (0, 0, 0);
    for seed in 1..=12 {
        let (_, w) = ChangesetWorkload::run(Seed::new(seed), FaultConfig::none(), 300)
            .unwrap_or_else(|v| panic!("{v}"));
        writes += w.writes;
        refusals += w.refusals;
        compared += w.compared;
    }
    assert!(writes > 1000, "only {writes} writes");
    assert!(refusals > 20, "only {refusals} refusals");
    assert!(compared > 3000, "only {compared} comparisons");
}
