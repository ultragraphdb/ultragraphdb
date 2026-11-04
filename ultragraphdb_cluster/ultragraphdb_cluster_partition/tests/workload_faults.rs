/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The partition workload's fixed seeds under every fault class but corruption, which a posting list cannot always
//! detect: partitions fail batches and reads, crash and lose power, recover from their logs while the others go on, and
//! every invariant holds after every step. A seed replays its run.

use ultragraphdb_store_sim::FaultConfig;
use ultragraphdb_tool_bdd::TestResult;
use ultragraphdb_tool_test::PartitionWorkload;

fn faults() -> FaultConfig {
    FaultConfig::mixed().with_corruption(0)
}

#[test]
fn test_seeds_with_faults_keep_every_invariant() -> TestResult {
    let mut recoveries = 0;
    let mut commits = 0;
    for seed in 100..=107 {
        let partitions = if seed % 2 == 0 { 2 } else { 3 };
        let counts = PartitionWorkload::run(seed, partitions, faults(), 300)?.counts();
        println!("seed {seed} over {partitions}: {counts:?}");
        assert!(counts.commits > 20, "seed {seed}: {counts:?}");
        assert!(counts.reverse_rows > 0, "seed {seed}: {counts:?}");
        recoveries += counts.recoveries;
        commits += counts.commits;
    }
    assert!(
        recoveries > 8,
        "only {recoveries} recoveries: the faults must reach the run"
    );
    assert!(commits > 400, "only {commits} commits");
    Ok(())
}

#[test]
fn test_a_seed_replays_its_run() -> TestResult {
    let first = PartitionWorkload::run(100, 2, faults(), 150)?.counts();
    let second = PartitionWorkload::run(100, 2, faults(), 150)?.counts();
    assert_eq!(first, second);
    Ok(())
}
