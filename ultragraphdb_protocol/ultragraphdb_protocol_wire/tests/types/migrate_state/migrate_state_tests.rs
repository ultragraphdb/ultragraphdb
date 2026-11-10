/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::MigrateState;
use ultragraphdb_tool_bdd::TestResult;

/// `PROTOCOL.md` §8.6: the states 1 `staged` to 9 `failed`.
#[test]
fn test_each_state_keeps_its_number() -> TestResult {
    let all = [
        MigrateState::Staged,
        MigrateState::Copying,
        MigrateState::Validating,
        MigrateState::Checked,
        MigrateState::Converging,
        MigrateState::Fenced,
        MigrateState::CatchingUp,
        MigrateState::CutOver,
        MigrateState::Failed,
    ];
    for (number, value) in (1u8..).zip(all) {
        assert_eq!(value.number(), number);
        assert_eq!(MigrateState::from_number(number), Some(value));
    }
    assert_eq!(MigrateState::from_number(0), None);
    assert_eq!(MigrateState::from_number(10), None);
    assert_eq!(MigrateState::from_number(255), None);
    Ok(())
}
