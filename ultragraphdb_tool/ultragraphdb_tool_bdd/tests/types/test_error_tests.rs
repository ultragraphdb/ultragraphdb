/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_tool_bdd::{TestError, TestResult};

#[test]
fn test_an_error_converts_with_its_text() -> TestResult {
    let Err(failure) = "not a number".parse::<u8>() else {
        return Err(TestError::new("the parse fails"));
    };
    let converted = TestError::from(failure.clone());
    assert_eq!(converted.reason(), failure.to_string());
    Ok(())
}

#[test]
fn test_question_mark_converts_an_error() -> TestResult {
    let parse = || -> TestResult<u8> { Ok("not a number".parse::<u8>()?) };
    let Err(failure) = "not a number".parse::<u8>() else {
        return Err(TestError::new("the parse fails"));
    };
    assert_eq!(parse(), Err(TestError::from(failure)));
    Ok(())
}

#[test]
fn test_an_error_displays_its_reason() -> TestResult {
    let error = TestError::new("what went wrong");
    assert_eq!(error.to_string(), error.reason());
    Ok(())
}
