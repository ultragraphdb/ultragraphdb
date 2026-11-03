/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::RoleKind;
use ultragraphdb_tool_bdd::TestResult;

#[test]
fn test_each_kind_has_its_number() -> TestResult {
    for (kind, number) in [
        (RoleKind::Frontend, 1),
        (RoleKind::Compute, 2),
        (RoleKind::Storage, 3),
    ] {
        assert_eq!(kind.number(), number);
        assert_eq!(RoleKind::from_number(number), Some(kind));
    }
    Ok(())
}

#[test]
fn test_a_number_no_kind_has_names_none() -> TestResult {
    for number in [0, 4, 255] {
        assert_eq!(RoleKind::from_number(number), None);
    }
    Ok(())
}
