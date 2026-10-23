/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_error_num::NumError;

#[test]
fn test_display_is_gos_text() {
    assert_eq!(
        NumError::Syntax("ParseInt", "\"12a\"".to_string()).to_string(),
        "strconv.ParseInt: parsing \"12a\": invalid syntax"
    );
    assert_eq!(
        NumError::Range("ParseInt", "\"9223372036854775808\"".to_string()).to_string(),
        "strconv.ParseInt: parsing \"9223372036854775808\": value out of range"
    );
    // The input is kept as Go's strconv.Quote writes it, escapes included.
    assert_eq!(
        NumError::Syntax("ParseFloat", "\"\\xff\"".to_string()).to_string(),
        "strconv.ParseFloat: parsing \"\\xff\": invalid syntax"
    );
}

#[test]
fn test_base_display_is_gos_text() {
    assert_eq!(
        NumError::Base("ParseUint", "\"10\"".to_string(), 1).to_string(),
        "strconv.ParseUint: parsing \"10\": invalid base 1"
    );
}
