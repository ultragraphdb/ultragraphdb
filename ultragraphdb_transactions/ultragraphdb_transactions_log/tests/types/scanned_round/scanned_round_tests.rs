/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_traits_log::Position;
use ultragraphdb_transactions_log::ScannedRound;

#[test]
fn test_fields_read_back_as_given() {
    let round = ScannedRound::new(Position::new(3), 0xA1, 0xB2, 40, 5, 8_192, 5_272);
    assert_eq!(round.position(), Position::new(3));
    assert_eq!(round.parent(), 0xA1);
    assert_eq!(round.checksum(), 0xB2);
    assert_eq!(round.first_version(), 40);
    assert_eq!(round.transactions(), 5);
    assert_eq!(round.offset(), 8_192);
    assert_eq!(round.size(), 5_272);
}

#[test]
fn test_padded_size_is_whole_blocks() {
    // A frame's padded size is its size rounded up to 4,096: 256 is one block, 4,096 one, 4,097 two.
    let padded = |size| ScannedRound::new(Position::new(1), 0, 0, 0, 0, 0, size).padded_size();
    assert_eq!(padded(256), 4_096);
    assert_eq!(padded(4_095), 4_096);
    assert_eq!(padded(4_096), 4_096);
    assert_eq!(padded(4_097), 8_192);
    assert_eq!(padded(5_272), 8_192);
    assert_eq!(padded(1_048_576), 1_048_576);
}
