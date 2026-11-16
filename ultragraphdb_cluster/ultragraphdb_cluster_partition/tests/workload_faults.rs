/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The partition workload's fixed seeds under every fault class but corruption, which a posting list cannot always
//! detect: partitions fail batches and reads, crash and lose power, recover from their logs while the others go on, and
//! every invariant holds after every step. A seed replays its run.
//!
//! Each seed is a test of its own, which `BUILD.bazel` runs as a target of its own, beside the others. Over the eight seeds
//! the faults reach the runs more than [`RECOVERIES_OVER`] times and more than [`COMMITS_OVER`] writes commit: each seed
//! takes more than its share of both, so the eight together do.

use ultragraphdb_store_sim::FaultConfig;
use ultragraphdb_tool_bdd::TestResult;
use ultragraphdb_tool_test::PartitionWorkload;

/// The seeds, each over two partitions or three by its parity.
const SEEDS: std::ops::RangeInclusive<u64> = 100..=107;
/// The recoveries the eight seeds' runs exceed together: the faults must reach the runs.
const RECOVERIES_OVER: u64 = 8;
/// The commits the eight seeds' runs exceed together.
const COMMITS_OVER: u64 = 400;

fn faults() -> FaultConfig {
    FaultConfig::mixed().with_corruption(0)
}

/// Runs `seed`, one of [`SEEDS`], and checks its counts: its own, and its share of the eight seeds'.
fn seed_keeps_every_invariant(seed: u64) -> TestResult {
    assert!(SEEDS.contains(&seed), "seed {seed} is not one of {SEEDS:?}");
    let seeds = SEEDS.end() - SEEDS.start() + 1;
    let partitions = if seed % 2 == 0 { 2 } else { 3 };
    let counts = PartitionWorkload::run(seed, partitions, faults(), 300)?.counts();
    println!("seed {seed} over {partitions}: {counts:?}");
    assert!(counts.commits > 20, "seed {seed}: {counts:?}");
    assert!(counts.reverse_rows > 0, "seed {seed}: {counts:?}");
    assert!(
        counts.recoveries > RECOVERIES_OVER / seeds,
        "seed {seed}: only {} recoveries: the faults must reach the run",
        counts.recoveries
    );
    assert!(
        counts.commits > COMMITS_OVER / seeds,
        "seed {seed}: only {} commits",
        counts.commits
    );
    Ok(())
}

#[test]
fn test_seed_100_with_faults_keeps_every_invariant() -> TestResult {
    seed_keeps_every_invariant(100)
}

#[test]
fn test_seed_101_with_faults_keeps_every_invariant() -> TestResult {
    seed_keeps_every_invariant(101)
}

#[test]
fn test_seed_102_with_faults_keeps_every_invariant() -> TestResult {
    seed_keeps_every_invariant(102)
}

#[test]
fn test_seed_103_with_faults_keeps_every_invariant() -> TestResult {
    seed_keeps_every_invariant(103)
}

#[test]
fn test_seed_104_with_faults_keeps_every_invariant() -> TestResult {
    seed_keeps_every_invariant(104)
}

#[test]
fn test_seed_105_with_faults_keeps_every_invariant() -> TestResult {
    seed_keeps_every_invariant(105)
}

#[test]
fn test_seed_106_with_faults_keeps_every_invariant() -> TestResult {
    seed_keeps_every_invariant(106)
}

#[test]
fn test_seed_107_with_faults_keeps_every_invariant() -> TestResult {
    seed_keeps_every_invariant(107)
}

#[test]
fn test_a_seed_replays_its_run() -> TestResult {
    let first = PartitionWorkload::run(100, 2, faults(), 150)?.counts();
    let second = PartitionWorkload::run(100, 2, faults(), 150)?.counts();
    assert_eq!(first, second);
    Ok(())
}
