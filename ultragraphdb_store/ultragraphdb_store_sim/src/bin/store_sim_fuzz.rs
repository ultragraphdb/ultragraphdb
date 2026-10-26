/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! Runs the store workload over fresh seeds, outside `make test`:
//! `store_sim_fuzz [--versionless-reads] [first seed] [seeds] [steps]`, the flag having the readers read the keyspace
//! without versions too.
//! Without a first seed it starts at one drawn from the wall clock. It prints every seed it runs and, for a failing
//! one, the violation; replaying a seed is running it again as the first seed with one seed, and with the flag when
//! it ran with it. The flag comes first, and each number is a whole number at its position among the numbers. A
//! first argument starting with `--` other than the flag, a non-numeric argument, a fourth number, zero seeds or
//! seeds running past `u64::MAX` print the refusal with the usage line and exit with 2, before any seed runs.

use std::process::ExitCode;

use ultragraphdb_store_sim::{FaultConfig, FuzzArgs, Seed, StoreWorkload};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args_os()
        .skip(1)
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    let run = match FuzzArgs::parse(&args, fresh_seed()) {
        Ok(run) => run,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(2);
        }
    };
    let failures = run
        .seeds_run()
        .filter(|&seed| {
            println!("seed {seed}");
            let workload = StoreWorkload::new(3);
            let workload = if run.versionless_reads() {
                workload.with_versionless_reads()
            } else {
                workload
            };
            match workload.run_on(Seed::new(seed), FaultConfig::mixed(), run.steps()) {
                Ok(_) => false,
                Err(violation) => {
                    println!("FAILED {violation}");
                    true
                }
            }
        })
        .count();
    println!("{failures} of {} seeds failed", run.seeds());
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
