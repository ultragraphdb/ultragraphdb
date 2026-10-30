/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::error::Error;

use ultragraphdb_error_invariant::InvariantError;

#[test]
fn test_it_is_an_error_with_no_source() {
    let error: &dyn Error = &InvariantError::Violated("a session has one owner");
    assert!(error.source().is_none());
    assert_eq!(
        error.to_string(),
        "invariant violated: a session has one owner"
    );
}
