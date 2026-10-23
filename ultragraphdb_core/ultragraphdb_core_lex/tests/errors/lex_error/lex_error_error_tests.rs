/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::error::Error;

use ultragraphdb_core_lex::Lexer;

#[test]
fn test_is_a_std_error_without_source() {
    let err = Lexer::new("").new_iterator().peek(1).unwrap_err();
    let err: &dyn Error = &err;
    assert!(err.source().is_none());
    assert_eq!(err.to_string(), "Out of range for peek");
}
