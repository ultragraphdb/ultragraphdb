/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::PatternSlot;

#[test]
fn test_slots_compare_by_kind_and_value() {
    assert_eq!(PatternSlot::Bound(0), PatternSlot::Bound(0));
    assert_ne!(PatternSlot::Bound(0), PatternSlot::Bound(1));
    assert_ne!(PatternSlot::Unbound, PatternSlot::DefaultGraph);
    assert_ne!(PatternSlot::Bound(0), PatternSlot::Parameter(0));
}
