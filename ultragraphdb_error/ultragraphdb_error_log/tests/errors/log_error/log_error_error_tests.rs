/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::error::Error;

use ultragraphdb_error_log::LogError;

#[test]
fn test_is_a_std_error_without_a_source() {
    let error: Box<dyn Error> = Box::new(LogError::Sealed(9));
    assert!(error.source().is_none());
    assert_eq!(error.to_string(), "the log was sealed at position 9");
}
