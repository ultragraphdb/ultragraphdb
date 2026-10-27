/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_traits_log::Position;

#[test]
fn test_zero_is_the_place_before_the_first_round() {
    assert_eq!(Position::ZERO.get(), 0);
    assert_eq!(Position::default(), Position::ZERO);
    assert_eq!(Position::ZERO.next(), Some(Position::new(1)));
}

#[test]
fn test_new_and_get_round_trip() {
    for n in [1, 2, 9, u64::MAX - 1, u64::MAX] {
        assert_eq!(Position::new(n).get(), n);
    }
}

#[test]
fn test_next_is_one_more_and_stops_at_the_last() {
    assert_eq!(Position::new(5).next(), Some(Position::new(6)));
    assert_eq!(
        Position::new(u64::MAX - 1).next(),
        Some(Position::new(u64::MAX))
    );
    assert_eq!(Position::new(u64::MAX).next(), None);
}

#[test]
fn test_positions_order_by_number() {
    assert!(Position::new(1) < Position::new(2));
    assert!(Position::ZERO < Position::new(1));
    assert!(Position::new(10) > Position::new(9));
}
