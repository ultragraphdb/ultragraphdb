/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// The workload's counts, for a test to see that a run exercised what it should.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct PartitionCounts {
    /// Writes committed.
    pub commits: u64,
    /// Writes refused, each for a reason a fault or a concurrent write explains.
    pub refusals: u64,
    /// Registrations the committed writes waited for.
    pub registrations: u64,
    /// Partitions recovered.
    pub recoveries: u64,
    /// Reverse reads compared with the reference model.
    pub reverse_reads: u64,
    /// Reverse reads that returned at least one subject.
    pub reverse_rows: u64,
    /// Transactions' scans compared with the reference model.
    pub scans: u64,
    /// In-doubt writes a recovery found applied.
    pub in_doubt_applied: u64,
}
