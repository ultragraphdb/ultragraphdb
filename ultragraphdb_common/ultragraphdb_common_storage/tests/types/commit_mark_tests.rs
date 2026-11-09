/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_common_storage::{CommitMark, LogIoTotals};

/// A mark reads back what it was made of, and stays within a cache line.
#[test]
fn test_a_mark_reads_back() {
    let log = LogIoTotals::new((5, 1, 4_096), (450, 1));
    let mark = CommitMark::new(1_000, log, 70);
    assert_eq!(mark.at_us(), 1_000);
    assert_eq!(mark.log(), &log);
    assert_eq!(mark.waited_us(), 70);
    assert!(size_of::<CommitMark>() <= 64);
}
