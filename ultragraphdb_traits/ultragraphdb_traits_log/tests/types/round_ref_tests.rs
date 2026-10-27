/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_traits_log::{Position, RoundRef};

#[test]
fn test_fields_read_back_as_given() {
    // Every field distinct, so a getter returning a neighbour is caught.
    let body = [1u8, 2, 3];
    let round = RoundRef::new(Position::new(7), 0xA1, 40, 3, &body);
    assert_eq!(round.position(), Position::new(7));
    assert_eq!(round.parent(), 0xA1);
    assert_eq!(round.first_version(), 40);
    assert_eq!(round.transactions(), 3);
    assert_eq!(round.body(), &[1, 2, 3]);
}

#[test]
fn test_an_empty_body_is_a_round() {
    let round = RoundRef::new(Position::new(1), 0, 0, 0, &[]);
    assert!(round.body().is_empty());
}
