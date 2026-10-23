/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::error::Error;

use ultragraphdb_error_num::NumError;

#[test]
fn test_is_a_std_error_without_a_source() {
    let e = NumError::Syntax("Atoi", "\"\"".to_string());
    assert!(e.source().is_none());
}
