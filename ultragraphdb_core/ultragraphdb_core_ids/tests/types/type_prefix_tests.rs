/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_ids::{IdError, IdErrorEnum, TypePrefix};
use ultragraphdb_tool_bdd::TestResult;

// The reserved prefixes, system-identity: 0 invalid, 1 Resource, 2 Statement.

#[test]
fn test_prefix_zero_is_refused() {
    assert_eq!(
        TypePrefix::new(0),
        Err(IdError(IdErrorEnum::ZeroTypePrefix))
    );
}

#[test]
fn test_the_built_in_prefixes_are_one_and_two() -> TestResult {
    assert_eq!(TypePrefix::RESOURCE.get(), 1);
    assert_eq!(TypePrefix::STATEMENT.get(), 2);
    assert_eq!(TypePrefix::new(1)?, TypePrefix::RESOURCE);
    assert_eq!(TypePrefix::new(2)?, TypePrefix::STATEMENT);
    Ok(())
}

#[test]
fn test_the_smallest_declared_and_the_largest_prefix_hold_their_value() -> TestResult {
    assert_eq!(TypePrefix::new(3)?.get(), 3);
    assert_eq!(TypePrefix::new(65_535)?.get(), 65_535);
    Ok(())
}

#[test]
fn test_prefixes_order_as_numbers() -> TestResult {
    assert!(TypePrefix::RESOURCE < TypePrefix::STATEMENT);
    assert!(TypePrefix::new(256)? > TypePrefix::new(255)?);
    Ok(())
}
