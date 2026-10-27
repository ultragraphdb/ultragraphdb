/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::PatternSlot;

#[test]
fn test_slots_compare_by_kind_and_value() {
    assert_eq!(PatternSlot::Bound("<a>"), PatternSlot::Bound("<a>"));
    assert_ne!(PatternSlot::Bound("<a>"), PatternSlot::Bound("<b>"));
    assert_ne!(PatternSlot::<&str>::Unbound, PatternSlot::DefaultGraph);
    assert_ne!(PatternSlot::<&str>::Parameter(0), PatternSlot::Parameter(1));
}
