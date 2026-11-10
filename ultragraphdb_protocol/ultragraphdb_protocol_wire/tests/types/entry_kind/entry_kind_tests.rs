/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::EntryKind;
use ultragraphdb_tool_bdd::TestResult;

/// `PROTOCOL.md` §8.3: the entry kinds 1 to 20, 0 and 21 naming none.
#[test]
fn test_each_entry_kind_keeps_its_number() -> TestResult {
    let all = [
        EntryKind::Schema,
        EntryKind::Module,
        EntryKind::Type,
        EntryKind::PrefixRanges,
        EntryKind::Attribute,
        EntryKind::Predicate,
        EntryKind::Role,
        EntryKind::Navigation,
        EntryKind::Enum,
        EntryKind::Record,
        EntryKind::NamedAttributeType,
        EntryKind::Quantity,
        EntryKind::Unit,
        EntryKind::Derivation,
        EntryKind::Function,
        EntryKind::Graph,
        EntryKind::Classification,
        EntryKind::Report,
        EntryKind::Source,
        EntryKind::RetiredName,
    ];
    for (number, value) in (1u8..).zip(all) {
        assert_eq!(value.number(), number);
        assert_eq!(EntryKind::from_number(number), Some(value));
    }
    assert_eq!(EntryKind::from_number(0), None);
    assert_eq!(EntryKind::from_number(21), None);
    assert_eq!(EntryKind::from_number(255), None);
    Ok(())
}
