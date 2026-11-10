/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::RetiredFate;
use ultragraphdb_tool_bdd::TestResult;

/// `PROTOCOL.md` §8.3: `fate` 1 renamed, 2 retired.
#[test]
fn test_each_fate_keeps_its_number() -> TestResult {
    let all = [RetiredFate::Renamed, RetiredFate::Retired];
    for (number, value) in (1u8..).zip(all) {
        assert_eq!(value.number(), number);
        assert_eq!(RetiredFate::from_number(number), Some(value));
    }
    assert_eq!(RetiredFate::from_number(0), None);
    assert_eq!(RetiredFate::from_number(3), None);
    assert_eq!(RetiredFate::from_number(255), None);
    Ok(())
}
