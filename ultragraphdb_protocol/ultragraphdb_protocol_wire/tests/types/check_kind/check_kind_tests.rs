/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::CheckKind;
use ultragraphdb_tool_bdd::TestResult;

/// `PROTOCOL.md` §13.24: `kind` 1 marks nothing, 2 may mark; no other number is a kind.
#[test]
fn test_each_kind_keeps_its_number() -> TestResult {
    assert_eq!(CheckKind::Asks.number(), 1);
    assert_eq!(CheckKind::Marks.number(), 2);
    for number in 0..=u8::MAX {
        let expected = match number {
            1 => Some(CheckKind::Asks),
            2 => Some(CheckKind::Marks),
            _ => None,
        };
        assert_eq!(CheckKind::from_number(number), expected, "{number}");
    }
    Ok(())
}
