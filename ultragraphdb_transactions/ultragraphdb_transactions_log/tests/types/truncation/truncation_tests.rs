/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_traits_log::Position;
use ultragraphdb_transactions_log::{Damage, Truncation};

#[test]
fn test_fields_read_back_as_given() {
    let damage = Damage::Chunk {
        position: Position::new(5),
        index: 1,
        start: 3_840,
        end: 5_000,
    };
    let truncation = Truncation::new(Position::new(5), 2, 8_192, damage);
    assert_eq!(truncation.position(), Position::new(5));
    assert_eq!(truncation.segment(), 2);
    assert_eq!(truncation.offset(), 8_192);
    assert_eq!(truncation.damage(), damage);
}
