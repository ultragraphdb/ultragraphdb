/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::ReferenceStatus;
use ultragraphdb_tool_bdd::TestResult;

/// `PROTOCOL.md` §13.15: `status` 1 absent, 2 live, 3 deleted, 4 retracted.
#[test]
fn test_each_status_keeps_its_number() -> TestResult {
    let all = [
        ReferenceStatus::Absent,
        ReferenceStatus::Live,
        ReferenceStatus::Deleted,
        ReferenceStatus::Retracted,
    ];
    for (number, value) in (1u8..).zip(all) {
        assert_eq!(value.number(), number);
        assert_eq!(ReferenceStatus::from_number(number), Some(value));
    }
    assert_eq!(ReferenceStatus::from_number(0), None);
    assert_eq!(ReferenceStatus::from_number(5), None);
    assert_eq!(ReferenceStatus::from_number(255), None);
    Ok(())
}
