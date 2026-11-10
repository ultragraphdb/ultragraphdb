/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A retired name as its sender holds it (`PROTOCOL.md` §8.3).

use ultragraphdb_protocol_wire::{RetiredFate, RetiredItem, RetiredName};
use ultragraphdb_tool_bdd::TestResult;

#[test]
fn test_a_renamed_name_holds_its_current_name_and_window() -> TestResult {
    let renamed = RetiredName::renamed(RetiredItem::Attribute, "gen5.Org.legal_name", 3)
        .until(1_700_000_000_000_000);
    assert_eq!(renamed.item(), RetiredItem::Attribute);
    assert_eq!(renamed.fate(), RetiredFate::Renamed);
    assert_eq!(renamed.current(), Some("gen5.Org.legal_name"));
    assert_eq!(renamed.version(), 3);
    assert_eq!(renamed.deprecated_until(), 1_700_000_000_000_000);
    Ok(())
}

#[test]
fn test_a_retired_name_holds_no_item_and_no_window() -> TestResult {
    let retired = RetiredName::retired(RetiredItem::Graph, 4);
    assert_eq!(retired.item(), RetiredItem::Graph);
    assert_eq!(retired.fate(), RetiredFate::Retired);
    assert_eq!(retired.current(), None);
    assert_eq!(retired.version(), 4);
    assert_eq!(retired.deprecated_until(), 0);
    Ok(())
}
