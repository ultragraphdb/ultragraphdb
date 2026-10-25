/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::collections::HashSet;

use ultragraphdb_error_num::{NumError, NumErrorEnum};

#[test]
fn test_constructors_build_their_variant() {
    assert_eq!(
        NumError::Syntax("ParseInt", r#""x""#.to_string()).0,
        NumErrorEnum::Syntax {
            func: "ParseInt",
            num: r#""x""#.to_string()
        }
    );
    assert_eq!(
        NumError::Range("ParseUint", r#""99""#.to_string()).0,
        NumErrorEnum::Range {
            func: "ParseUint",
            num: r#""99""#.to_string()
        }
    );
    assert_eq!(
        NumError::Base("ParseInt", r#""10""#.to_string(), 37).0,
        NumErrorEnum::Base {
            func: "ParseInt",
            num: r#""10""#.to_string(),
            base: 37
        }
    );
}

#[test]
fn test_the_classification_is_the_public_field() {
    let classify = |e: &NumError| match &e.0 {
        NumErrorEnum::Syntax { func, num } => format!("syntax {func} {num}"),
        NumErrorEnum::Range { func, num } => format!("range {func} {num}"),
        NumErrorEnum::Base { func, num, base } => format!("base {func} {num} {base}"),
    };
    assert_eq!(
        classify(&NumError::Syntax("ParseBool", r#""yes""#.to_string())),
        r#"syntax ParseBool "yes""#
    );
    assert_eq!(
        classify(&NumError::Range("Atoi", r#""1""#.to_string())),
        r#"range Atoi "1""#
    );
    assert_eq!(
        classify(&NumError::Base("ParseUint", r#""0""#.to_string(), 1)),
        r#"base ParseUint "0" 1"#
    );
}

#[test]
fn test_every_field_distinguishes_two_errors() {
    // Callers compare errors to Go's expected NumError, so the variant, the function, the input and the base must
    // each tell two errors apart; equal errors hash alike.
    let distinct = [
        NumError::Syntax("ParseInt", r#""1""#.to_string()),
        NumError::Range("ParseInt", r#""1""#.to_string()),
        NumError::Base("ParseInt", r#""1""#.to_string(), 1),
        NumError::Syntax("ParseUint", r#""1""#.to_string()),
        NumError::Syntax("ParseInt", r#""2""#.to_string()),
        NumError::Base("ParseInt", r#""1""#.to_string(), 37),
    ];
    for (i, a) in distinct.iter().enumerate() {
        for (j, b) in distinct.iter().enumerate() {
            assert_eq!(a == b, i == j, "{a} vs {b}");
        }
    }
    let set: HashSet<NumError> = distinct.iter().chain(distinct.iter()).cloned().collect();
    assert_eq!(set.len(), distinct.len());
}
