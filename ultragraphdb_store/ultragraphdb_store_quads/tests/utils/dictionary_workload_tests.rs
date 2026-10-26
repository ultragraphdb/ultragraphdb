/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_store_sim::{FaultConfig, Seed};

use crate::utils::dictionary_workload::DictionaryWorkload;

#[test]
fn test_seeds_without_faults() {
    let mut checked = 0;
    for seed in 1..=8 {
        let (_, c) = DictionaryWorkload::run(Seed::new(seed), FaultConfig::none(), 200)
            .unwrap_or_else(|v| panic!("{v}"));
        checked += c;
    }
    assert!(checked > 500, "only {checked} lists checked");
}

#[test]
fn test_seeds_with_faults() {
    let mut checked = 0;
    for seed in 100..=115 {
        let (_, c) = DictionaryWorkload::run(Seed::new(seed), DictionaryWorkload::faults(), 200)
            .unwrap_or_else(|v| panic!("{v}"));
        checked += c;
    }
    assert!(checked > 1000, "only {checked} lists checked");
}
