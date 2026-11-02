/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_keys::CountDirection;

#[test]
fn test_directions_read_back_from_their_bytes() {
    // The bytes the keys record: 0 forward, 1 reverse.
    assert_eq!(CountDirection::Forward.byte(), 0);
    assert_eq!(CountDirection::Reverse.byte(), 1);
    for d in [CountDirection::Forward, CountDirection::Reverse] {
        assert_eq!(CountDirection::from_byte(d.byte()), Some(d));
    }
    assert_eq!(CountDirection::from_byte(2), None);
}
