/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The partition workload over partitions that keep every invariant: its counts show each check ran. The partition crate's
//! suite runs it over more seeds and longer.

use ultragraphdb_store_sim::FaultConfig;
use ultragraphdb_tool_bdd::TestResult;
use ultragraphdb_tool_test::PartitionWorkload;

#[test]
fn test_every_scan_is_read_again_by_pages_plain_and_by_a_transaction() -> TestResult {
    let counts = PartitionWorkload::run(1, 2, FaultConfig::none(), 150)?.counts();
    assert!(counts.scans > 0, "{counts:?}");
    // Each scan compared is read again twice, by plain pages and by a transaction's.
    assert_eq!(counts.paged_scans, 2 * counts.scans, "{counts:?}");
    assert!(counts.commits > 0, "{counts:?}");
    assert!(counts.reverse_rows > 0, "{counts:?}");
    assert_eq!(counts.recoveries, 0, "{counts:?}");
    Ok(())
}

#[test]
fn test_a_run_with_faults_recovers_and_still_scans() -> TestResult {
    let faults = FaultConfig::mixed().with_corruption(0);
    // One partition, so every recovery is the same node's: each start after the first takes tickets and a random source of
    // its own.
    let counts = PartitionWorkload::run(102, 1, faults, 300)?.counts();
    assert!(counts.recoveries >= 2, "{counts:?}");
    assert!(counts.scans > 0, "{counts:?}");
    assert_eq!(counts.paged_scans, 2 * counts.scans, "{counts:?}");
    Ok(())
}
