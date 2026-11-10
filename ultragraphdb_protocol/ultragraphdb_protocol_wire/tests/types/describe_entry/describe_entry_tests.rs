/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A Describe entry as its sender holds it (`PROTOCOL.md` §8.3).

use ultragraphdb_protocol_wire::{DescribeEntry, EntryKind, RetiredItem, RetiredName};
use ultragraphdb_tool_bdd::TestResult;

#[test]
fn test_an_entry_holds_its_kind_name_and_bytes() -> TestResult {
    let source = DescribeEntry::new(EntryKind::Source, "gen5", b"namespace gen5 {}".to_vec());
    assert_eq!(source.kind(), EntryKind::Source);
    assert_eq!(source.name(), "gen5");
    assert_eq!(source.bytes(), b"namespace gen5 {}");
    assert!(source.retired_name().is_none());
    let renamed = RetiredName::renamed(RetiredItem::Type, "gen5.Org", 3);
    let retired = DescribeEntry::retired("gen5.Company", renamed.clone());
    assert_eq!(retired.kind(), EntryKind::RetiredName);
    assert_eq!(retired.name(), "gen5.Company");
    assert!(retired.bytes().is_empty());
    assert_eq!(retired.retired_name(), Some(&renamed));
    Ok(())
}
