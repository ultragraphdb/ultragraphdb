/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_traits_log::Position;
use ultragraphdb_transactions_log::{Damage, ScannedRound, SegmentScan};

#[test]
fn test_fields_read_back_as_given() {
    let round = ScannedRound::new(Position::new(1), 0, 0xB2, 2, 1, 0, 1_256);
    let damage = Damage::Chain {
        position: Position::new(3),
    };
    let scan = SegmentScan::new(vec![round], 4_096, Some(damage));
    assert_eq!(scan.rounds(), &[round]);
    assert_eq!(scan.end(), 4_096);
    assert_eq!(scan.damage(), Some(damage));
    let clean = SegmentScan::new(Vec::new(), 0, None);
    assert!(clean.rounds().is_empty());
    assert_eq!(clean.end(), 0);
    assert_eq!(clean.damage(), None);
}
