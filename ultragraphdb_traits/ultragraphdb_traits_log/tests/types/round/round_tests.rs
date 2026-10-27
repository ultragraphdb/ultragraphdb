/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_traits_log::{Position, Round, RoundRef};

#[test]
fn test_fields_read_back_as_given() {
    let round = Round::new(Position::new(7), 0xA1, 0xC3, 40, 3, vec![9, 8]);
    assert_eq!(round.position(), Position::new(7));
    assert_eq!(round.parent(), 0xA1);
    assert_eq!(round.checksum(), 0xC3);
    assert_eq!(round.first_version(), 40);
    assert_eq!(round.transactions(), 3);
    assert_eq!(round.body(), &[9, 8]);
}

#[test]
fn test_as_round_ref_is_the_round_as_appended() {
    let round = Round::new(Position::new(7), 0xA1, 0xC3, 40, 3, vec![9, 8]);
    assert_eq!(
        round.as_round_ref(),
        RoundRef::new(Position::new(7), 0xA1, 40, 3, &[9, 8])
    );
}
