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

#[test]
fn test_the_largest_write_set_is_what_a_lone_round_holds_on_either_drive() {
    use ultragraphdb_transactions_log::WalConfig;
    // Worked by hand from PROTOCOL.md §2.3 and §12.1: a frame is the 256-byte header, the body's data and 8 bytes a
    // chunk once there are two or more; the data is the 24-byte section table, one 16-byte decision, one 8-byte record
    // entry and the record; chunk 0 holds 3,840 bytes and every later one 4,096.
    // 1,048,576 bytes (no power-loss protection): 256 chunks, 1,046,272 bytes of data, a record of 1,046,224.
    // 262,144 bytes (power-loss protection): 64 chunks, 261,376 bytes of data, a record of 261,328.
    for (protection, expected) in [(false, 1_046_224usize), (true, 261_328)] {
        let config = WalConfig::new(0, protection);
        let bounds = RoundBounds::new(config.round_bytes(), config.round_transactions());
        assert_eq!(bounds.max_write_set(), expected, "protection {protection}");
        assert_eq!(
            crate::utils::frames::largest_record(config.round_bytes() as usize),
            expected,
            "the wire's builder agrees"
        );
    }
}

#[test]
fn test_the_largest_write_set_follows_the_builder_at_chunk_boundaries_and_is_zero_below_any_round() {
    use crate::utils::frames::largest_record;
    // An empty record's lone round is 304 bytes (256 + 48): below it no record fits.
    assert_eq!(RoundBounds::new(0, 1).max_write_set(), 0);
    assert_eq!(RoundBounds::new(303, 1).max_write_set(), 0);
    // Around the first chunk's end (3,840 bytes of data, frames of 4,096) and the second's, where a table appears.
    for bytes in [304u32, 305, 4_095, 4_096, 4_097, 4_111, 4_112, 4_113, 8_200, 8_208, 100_000] {
        assert_eq!(
            RoundBounds::new(bytes, 1).max_write_set(),
            largest_record(bytes as usize),
            "{bytes}"
        );
    }
}
