/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::error::Error;

use ultragraphdb_error_num::NumError;

#[test]
fn test_is_a_std_error_without_a_source() {
    let errors = [
        NumError::Syntax("Atoi", r#""""#.to_string()),
        NumError::Range("ParseInt", r#""9223372036854775808""#.to_string()),
        NumError::Base("ParseUint", r#""10""#.to_string(), 1),
    ];
    for e in errors {
        assert!(e.source().is_none(), "{e}");
    }
}

#[test]
fn test_boxed_as_a_std_error_keeps_gos_text() {
    let e: Box<dyn Error> = Box::new(NumError::Range("ParseFloat", r#""1e309""#.to_string()));
    assert_eq!(
        e.to_string(),
        r#"strconv.ParseFloat: parsing "1e309": value out of range"#
    );
    assert_eq!(
        e.downcast_ref::<NumError>(),
        Some(&NumError::Range("ParseFloat", r#""1e309""#.to_string()))
    );
}
