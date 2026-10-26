/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The fixed seed set of the changeset workload: without faults, the store compared with the model after every step;
//! with every fault class; and a seed's replay.

use ultragraphdb_store_sim::{FaultConfig, Seed};

use crate::utils::changeset_workload::ChangesetWorkload;

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

#[test]
fn test_a_seed_replays_its_run() {
    let run = || {
        let (outcome, w) =
            ChangesetWorkload::run(Seed::new(7), ChangesetWorkload::faults(), 200).expect("run");
        (format!("{outcome:?}"), w.writes, w.checked)
    };
    assert_eq!(run(), run());
}
