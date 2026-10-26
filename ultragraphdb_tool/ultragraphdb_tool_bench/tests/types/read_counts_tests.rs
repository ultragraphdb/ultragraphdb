/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_tool_bench::ReadCounts;

#[test]
fn test_lookups_hits_and_store_reads_are_counted_apart() {
    let counts = ReadCounts::new();
    assert_eq!(counts.get(), (0, 0, 0, 0));
    counts.lookup(true);
    counts.lookup(false);
    counts.lookup(true);
    counts.store_read();
    counts.store_read();
    counts.empty_read();
    assert_eq!(counts.get(), (3, 2, 2, 1));
}
