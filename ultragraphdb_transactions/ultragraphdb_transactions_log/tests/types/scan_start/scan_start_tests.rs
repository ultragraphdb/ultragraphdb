/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_traits_log::Position;
use ultragraphdb_transactions_log::ScanStart;

#[test]
fn test_the_first_segment_kept_has_no_known_first_round() {
    let start = ScanStart::new(11, 2, Position::new(9), 0xC9);
    assert_eq!(start.cluster(), 11);
    assert_eq!(start.incarnation(), 2);
    assert_eq!(start.checkpoint(), Position::new(9));
    assert_eq!(start.checkpoint_checksum(), 0xC9);
    assert_eq!(start.next(), None);
}

#[test]
fn test_continuing_names_the_first_round_and_keeps_the_rest() {
    let start = ScanStart::new(11, 2, Position::new(9), 0xC9).continuing(Position::new(14), 0xD3);
    assert_eq!(start.next(), Some((Position::new(14), 0xD3)));
    assert_eq!(start.cluster(), 11);
    assert_eq!(start.incarnation(), 2);
    assert_eq!(start.checkpoint(), Position::new(9));
    assert_eq!(start.checkpoint_checksum(), 0xC9);
}
