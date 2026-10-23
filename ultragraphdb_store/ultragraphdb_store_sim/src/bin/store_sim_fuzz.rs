/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Runs the store workload over fresh seeds, outside `make test`: `store_sim_fuzz [first seed] [seeds] [steps]`.
//! Without a first seed it starts at one drawn from the wall clock. It prints every seed it runs and, for a failing
//! one, the violation; replaying a seed is running it again as the first seed with one seed.

use std::process::ExitCode;

use ultragraphdb_store_sim::{FaultConfig, Seed, StoreWorkload};

fn main() -> ExitCode {
    let args: Vec<u64> = std::env::args()
        .skip(1)
        .filter_map(|a| a.parse().ok())
        .collect();
    let first = args.first().copied().unwrap_or_else(fresh_seed);
    let seeds = args.get(1).copied().unwrap_or(1000);
    let steps = args.get(2).copied().unwrap_or(2000);
    let failures = (first..first.saturating_add(seeds))
        .filter(|&seed| {
            println!("seed {seed}");
            match StoreWorkload::run(Seed::new(seed), FaultConfig::mixed(), steps, 3) {
                Ok(_) => false,
                Err(violation) => {
                    println!("FAILED {violation}");
                    true
                }
            }
        })
        .count();
    println!("{failures} of {seeds} seeds failed");
    if failures == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// A seed no earlier run is likely to have used.
#[allow(
    clippy::disallowed_methods,
    reason = "the fuzz binary draws fresh seeds from the wall clock and prints each, so every run replays"
)]
fn fresh_seed() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos() as u64)
}
