/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_role::{RoleError, RoleErrorEnum};

/// The codes are the `sysexits` convention's, as the role-binaries spec's table states them.
#[test]
fn test_each_cause_exits_with_its_code_and_names_itself() {
    let configuration = RoleError::configuration("the setting a.b");
    assert_eq!(configuration.exit_code(), 78);
    assert!(configuration.to_string().contains("a.b"));
    assert!(matches!(
        configuration.0,
        RoleErrorEnum::Configuration { .. }
    ));
    let unavailable = RoleError::unavailable("no address");
    assert_eq!(unavailable.exit_code(), 69);
    assert!(unavailable.to_string().contains("no address"));
    let source: &dyn std::error::Error = &unavailable;
    assert!(source.source().is_none());
}
