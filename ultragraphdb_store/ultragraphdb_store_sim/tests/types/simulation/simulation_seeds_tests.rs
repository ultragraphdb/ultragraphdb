/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The fixed seed set: every fault class over the store workload, run in `make test`. Longer runs over fresh seeds
//! are the `store_sim_fuzz` binary's.

use ultragraphdb_store_sim::{FaultConfig, Seed, StoreWorkload};

#[test]
fn test_the_fixed_seeds_break_no_invariant() {
    for seed in 0..48 {
        if let Err(violation) = StoreWorkload::run(Seed::new(seed), FaultConfig::mixed(), 300, 2) {
            panic!("{violation}");
        }
    }
}

#[test]
fn test_the_fixed_seeds_break_no_invariant_without_faults() {
    for seed in 100..116 {
        if let Err(violation) = StoreWorkload::run(Seed::new(seed), FaultConfig::none(), 300, 3) {
            panic!("{violation}");
        }
    }
}
