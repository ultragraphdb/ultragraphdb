/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::num::ParseIntError;

use ultragraphdb_tool_bdd::{Context, TestError, TestResult};

/// A failed parse, the `Err` a test would otherwise `expect`.
fn failed_parse() -> Result<u8, ParseIntError> {
    "not a number".parse::<u8>()
}

#[test]
fn test_a_present_value_is_returned() -> TestResult {
    let value = 7_u8;
    assert_eq!(Some(value).context("the value")?, value);
    Ok(())
}

#[test]
fn test_an_absent_value_names_what_and_the_caller_s_line() -> TestResult {
    let what = "the value";
    let line = line!() + 1;
    let Err(e) = None::<u8>.context(what) else {
        return Err(TestError::new("an absent value is an error"));
    };
    assert!(e.reason().starts_with(&format!("{what}: none (at ")), "{e}");
    assert!(e.reason().contains(&format!("{}:{line}:", file!())), "{e}");
    Ok(())
}

#[test]
fn test_a_successful_result_is_returned() -> TestResult {
    let value = 7_u8;
    assert_eq!(Ok::<u8, ParseIntError>(value).context("the value")?, value);
    Ok(())
}

#[test]
fn test_a_failed_result_names_what_its_failure_and_the_caller_s_line() -> TestResult {
    let what = "the parse";
    let Err(failure) = failed_parse() else {
        return Err(TestError::new("the parse fails"));
    };
    let line = line!() + 1;
    let Err(e) = failed_parse().context(what) else {
        return Err(TestError::new("a failed result is an error"));
    };
    assert!(
        e.reason().starts_with(&format!("{what}: {failure} (at ")),
        "{e}"
    );
    assert!(e.reason().contains(&format!("{}:{line}:", file!())), "{e}");
    Ok(())
}
