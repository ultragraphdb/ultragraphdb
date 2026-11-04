/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The partition workload's fixed seeds without faults: over two and four partitions, every invariant holds after every
//! step and every reverse read at a closed epoch answers what a reference model of one partition answers.

use ultragraphdb_store_sim::FaultConfig;
use ultragraphdb_tool_bdd::TestResult;
use ultragraphdb_tool_test::PartitionWorkload;

#[test]
fn test_seeds_without_faults_keep_every_invariant() -> TestResult {
    for (seed, partitions) in [(1, 2), (2, 2), (3, 4), (4, 4)] {
        let counts = PartitionWorkload::run(seed, partitions, FaultConfig::none(), 300)?.counts();
        println!("seed {seed} over {partitions}: {counts:?}");
        // Without faults nothing crashes, and a run does what the test is for.
        assert_eq!(counts.recoveries, 0, "seed {seed}");
        assert!(counts.commits > 50, "seed {seed}: {counts:?}");
        assert!(counts.registrations > 0, "seed {seed}: {counts:?}");
        assert!(counts.reverse_rows > 0, "seed {seed}: {counts:?}");
    }
    Ok(())
}
