/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::RetiredItem;
use ultragraphdb_tool_bdd::TestResult;

/// `PROTOCOL.md` §8.3: `item` 1 a type, 2 an attribute, 3 a role, 4 a navigation name, 5 a graph.
#[test]
fn test_each_item_keeps_its_number() -> TestResult {
    let all = [
        RetiredItem::Type,
        RetiredItem::Attribute,
        RetiredItem::Role,
        RetiredItem::Navigation,
        RetiredItem::Graph,
    ];
    for (number, value) in (1u8..).zip(all) {
        assert_eq!(value.number(), number);
        assert_eq!(RetiredItem::from_number(number), Some(value));
    }
    assert_eq!(RetiredItem::from_number(0), None);
    assert_eq!(RetiredItem::from_number(6), None);
    assert_eq!(RetiredItem::from_number(255), None);
    Ok(())
}
