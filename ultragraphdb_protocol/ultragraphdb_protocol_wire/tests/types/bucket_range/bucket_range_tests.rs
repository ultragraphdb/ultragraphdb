/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::BucketRange;
use ultragraphdb_tool_bdd::TestResult;

#[test]
fn test_a_range_encodes_start_end_and_partition() -> TestResult {
    let range = BucketRange::new(0x0102_0304, 0x0506_0708, 0x0A0B_0C0D);
    assert_eq!(
        (range.start(), range.end(), range.partition()),
        (0x0102_0304, 0x0506_0708, 0x0A0B_0C0D)
    );
    assert_eq!(
        range.encode(),
        [4, 3, 2, 1, 8, 7, 6, 5, 0x0D, 0x0C, 0x0B, 0x0A, 0, 0, 0, 0]
    );
    assert_eq!(BucketRange::SIZE, 16);
    Ok(())
}
