/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::MigrateAction;
use ultragraphdb_tool_bdd::TestResult;

/// `PROTOCOL.md` §8.6: `action` 1 start, 2 dry run, 3 cut over, 4 cancel, 5 read the state.
#[test]
fn test_each_action_keeps_its_number() -> TestResult {
    let all = [
        MigrateAction::Start,
        MigrateAction::DryRun,
        MigrateAction::CutOver,
        MigrateAction::Cancel,
        MigrateAction::ReadState,
    ];
    for (number, value) in (1u8..).zip(all) {
        assert_eq!(value.number(), number);
        assert_eq!(MigrateAction::from_number(number), Some(value));
    }
    assert_eq!(MigrateAction::from_number(0), None);
    assert_eq!(MigrateAction::from_number(6), None);
    assert_eq!(MigrateAction::from_number(255), None);
    Ok(())
}
