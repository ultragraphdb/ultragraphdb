/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_keys::CountDirection;

#[test]
fn test_directions_read_back_from_their_bytes() {
    for d in [CountDirection::Forward, CountDirection::Reverse] {
        assert_eq!(CountDirection::from_byte(d.byte()), Some(d));
    }
    assert_eq!(CountDirection::from_byte(2), None);
}
