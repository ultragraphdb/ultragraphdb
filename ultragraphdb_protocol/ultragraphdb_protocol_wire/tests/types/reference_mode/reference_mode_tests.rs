/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::ReferenceMode;
use ultragraphdb_tool_bdd::TestResult;

/// `PROTOCOL.md` §13.15: `mode` 0 get or create, 1 get-only, 2 naming none.
#[test]
fn test_each_mode_keeps_its_number() -> TestResult {
    let all = [ReferenceMode::GetOrCreate, ReferenceMode::GetOnly];
    for (number, mode) in (0u8..).zip(all) {
        assert_eq!(mode.number(), number);
        assert_eq!(ReferenceMode::from_number(number), Some(mode));
    }
    assert_eq!(ReferenceMode::from_number(2), None);
    assert_eq!(ReferenceMode::from_number(255), None);
    Ok(())
}
