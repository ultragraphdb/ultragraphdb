/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The partition workload's fixed seeds without faults: over two and four partitions, every invariant holds after every
//! step and every reverse read at a closed epoch answers what a reference model of one partition answers. Each seed is a
//! test of its own, which `BUILD.bazel` runs as a target of its own, beside the others.

use ultragraphdb_store_sim::FaultConfig;
use ultragraphdb_tool_bdd::TestResult;
use ultragraphdb_tool_test::PartitionWorkload;

/// Runs `seed` over `partitions` without faults and checks its counts.
fn seed_keeps_every_invariant(seed: u64, partitions: u32) -> TestResult {
    let counts = PartitionWorkload::run(seed, partitions, FaultConfig::none(), 300)?.counts();
    println!("seed {seed} over {partitions}: {counts:?}");
    // Without faults nothing crashes, and a run does what the test is for.
    assert_eq!(counts.recoveries, 0, "seed {seed}");
    assert!(counts.commits > 50, "seed {seed}: {counts:?}");
    assert!(counts.registrations > 0, "seed {seed}: {counts:?}");
    assert!(counts.reverse_rows > 0, "seed {seed}: {counts:?}");
    assert!(counts.paged_scans > 0, "seed {seed}: {counts:?}");
    Ok(())
}

#[test]
fn test_seed_1_over_two_partitions_keeps_every_invariant() -> TestResult {
    seed_keeps_every_invariant(1, 2)
}

#[test]
fn test_seed_2_over_two_partitions_keeps_every_invariant() -> TestResult {
    seed_keeps_every_invariant(2, 2)
}

#[test]
fn test_seed_3_over_four_partitions_keeps_every_invariant() -> TestResult {
    seed_keeps_every_invariant(3, 4)
}

#[test]
fn test_seed_4_over_four_partitions_keeps_every_invariant() -> TestResult {
    seed_keeps_every_invariant(4, 4)
}
