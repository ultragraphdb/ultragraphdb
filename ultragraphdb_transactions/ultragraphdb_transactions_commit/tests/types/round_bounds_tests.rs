/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::MAX_FRAME_SIZE;
use ultragraphdb_transactions_commit::RoundBounds;

#[test]
fn test_bounds_within_the_frame_are_kept() {
    let bounds = RoundBounds::new(5_000, 9);
    assert_eq!(bounds.bytes(), 5_000);
    assert_eq!(bounds.transactions(), 9);
    let frame = RoundBounds::new(MAX_FRAME_SIZE as u32, 1);
    assert_eq!(frame.bytes(), 1_048_576);
}

#[test]
fn test_bytes_past_the_frame_are_the_frame_and_no_transactions_is_one() {
    let bounds = RoundBounds::new(MAX_FRAME_SIZE as u32 + 1, 0);
    assert_eq!(bounds.bytes(), 1_048_576);
    assert_eq!(bounds.transactions(), 1);
    assert_eq!(RoundBounds::new(u32::MAX, 2).bytes(), 1_048_576);
}
