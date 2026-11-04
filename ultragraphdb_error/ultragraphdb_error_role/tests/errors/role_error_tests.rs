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

/// A failed log and a broken invariant exit with their own codes, each naming what happened.
#[test]
fn test_each_stop_has_its_code_and_its_words() {
    let io = RoleError::io("the log");
    assert_eq!(io.exit_code(), RoleError::EX_IOERR);
    assert!(io.to_string().contains("the log"));
    let software = RoleError::software("an invariant");
    assert_eq!(software.exit_code(), RoleError::EX_SOFTWARE);
    assert!(software.to_string().contains("an invariant"));
}
