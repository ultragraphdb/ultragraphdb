/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::error::Error;

use ultragraphdb_error_json::JsonError;

#[test]
fn test_is_a_std_error_without_a_source() {
    let e = JsonError::Syntax("unexpected end of JSON input".to_string(), 0);
    assert!(e.source().is_none());
}
