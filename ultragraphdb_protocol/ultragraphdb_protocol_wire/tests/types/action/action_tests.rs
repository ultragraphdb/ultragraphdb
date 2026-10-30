/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::Action;

#[test]
fn test_actions_keep_their_numbers() {
    // SECURITY.md's actions, the detail of PermissionDenied.
    let numbers: Vec<u64> = Action::ALL.iter().map(|a| a.number()).collect();
    assert_eq!(numbers, vec![1, 2, 3, 4, 5, 6]);
    assert_eq!(Action::Read.number(), 1);
    assert_eq!(Action::Write.number(), 2);
    assert_eq!(Action::Declare.number(), 3);
    assert_eq!(Action::Drop.number(), 4);
    assert_eq!(Action::Admin.number(), 5);
    assert_eq!(Action::Operate.number(), 6);
    for action in Action::ALL {
        assert_eq!(Action::from_number(action.number()), Some(action));
    }
    assert_eq!(Action::from_number(0), None);
    assert_eq!(Action::from_number(7), None);
    assert_eq!(Action::from_number(u64::MAX), None);
}
