/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The changeset workload's fixed seeds with every fault class: every index agrees with the data after every step and
//! every recovery.

use ultragraphdb_store_sim::Seed;
use ultragraphdb_tool_test::ChangesetWorkload;

#[test]
fn test_seeds_with_faults_keep_every_index() {
    let (mut checked, mut writes) = (0, 0);
    for seed in 100..=111 {
        let (_, w) = ChangesetWorkload::run(Seed::new(seed), ChangesetWorkload::faults(), 300)
            .unwrap_or_else(|v| panic!("{v}"));
        checked += w.checked;
        writes += w.writes;
    }
    assert!(checked > 1000, "only {checked} checks");
    assert!(writes > 1000, "only {writes} writes");
}
