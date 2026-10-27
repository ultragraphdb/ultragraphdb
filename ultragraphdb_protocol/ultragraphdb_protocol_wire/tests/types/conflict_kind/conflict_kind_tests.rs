/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::ConflictKind;

#[test]
fn test_conflict_kinds_keep_their_numbers() {
    let numbers: Vec<u64> = ConflictKind::ALL.iter().map(|k| k.number()).collect();
    assert_eq!(numbers, vec![1, 2, 3, 4, 5, 6]);
    assert_eq!(ConflictKind::Range.number(), 3);
    for kind in ConflictKind::ALL {
        assert_eq!(ConflictKind::from_number(kind.number()), Some(kind));
    }
    assert_eq!(ConflictKind::from_number(0), None);
    assert_eq!(ConflictKind::from_number(7), None);
}
