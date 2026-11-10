/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! A `BodyError` is a standard error, with no source: it is the refusal itself.

use std::error::Error;

use ultragraphdb_core_body::BodyError;
use ultragraphdb_tool_bdd::TestResult;

#[test]
fn test_a_body_error_is_an_error_without_a_source() -> TestResult {
    let error = BodyError::trailing_bytes(9, 10);
    assert!(error.source().is_none());
    let boxed: Box<dyn Error> = Box::new(error);
    assert_eq!(
        boxed.to_string(),
        "a body holds 10 bytes, past the end of its last section at 9"
    );
    Ok(())
}
