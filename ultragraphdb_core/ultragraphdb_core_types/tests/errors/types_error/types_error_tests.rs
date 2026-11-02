/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::error::Error;

use ultragraphdb_core_types::{TypeId, TypesError, TypesErrorEnum, Val, convert};
use ultragraphdb_error_num::NumError;
use ultragraphdb_tool_bdd::{Context, TestResult};

#[test]
fn test_kind_and_message() -> TestResult {
    let err = convert(&Val::bytes(TypeId::STRING, b"x".to_vec()), TypeId::INT)
        .err()
        .context("expected an error")?;
    assert_eq!(
        err.0,
        TypesErrorEnum::Number(r#"strconv.ParseInt: parsing "x": invalid syntax"#.into())
    );
    assert_eq!(
        err.message(),
        r#"strconv.ParseInt: parsing "x": invalid syntax"#
    );
    assert_eq!(
        err.to_string(),
        r#"strconv.ParseInt: parsing "x": invalid syntax"#
    );
    let uid_err = convert(&Val::bytes(TypeId::INT, Vec::new()), TypeId::UID)
        .err()
        .context("expected an error")?;
    assert_eq!(
        uid_err.0,
        TypesErrorEnum::Conversion("invalid data for int64 []".into())
    );
    let e: &dyn Error = &uid_err;
    assert_eq!(e.to_string(), "invalid data for int64 []");
    assert!(e.source().is_none());
    Ok(())
}

#[test]
fn test_every_constructor_builds_its_variant_and_displays_its_message() {
    type Case = (fn(String) -> TypesError, fn(String) -> TypesErrorEnum);
    let cases: [Case; 7] = [
        (TypesError::conversion, TypesErrorEnum::Conversion),
        (TypesError::number, TypesErrorEnum::Number),
        (TypesError::time, TypesErrorEnum::Time),
        (TypesError::big_float, TypesErrorEnum::BigFloat),
        (TypesError::geo, TypesErrorEnum::Geo),
        (TypesError::password, TypesErrorEnum::Password),
        (TypesError::compare, TypesErrorEnum::Compare),
    ];
    for (i, (build, variant)) in cases.iter().enumerate() {
        for msg in ["", "a message", "ünïcode: 日本", "line\nbreak"] {
            let err = build(msg.to_string());
            assert_eq!(err.0, variant(msg.to_string()), "constructor {i}");
            assert_eq!(err.message(), msg, "constructor {i}");
            assert_eq!(err.to_string(), msg, "constructor {i}");
            assert!(err.source().is_none());
        }
    }
    // The classification tells apart errors of one text.
    assert_ne!(TypesError::geo("x"), TypesError::compare("x"));
}

#[test]
fn test_a_num_error_is_a_number_error_with_gos_text() {
    let cases = [
        (
            NumError::syntax("ParseInt", r#""x""#.into()),
            r#"strconv.ParseInt: parsing "x": invalid syntax"#,
        ),
        (
            NumError::range("ParseFloat", r#""1e400""#.into()),
            r#"strconv.ParseFloat: parsing "1e400": value out of range"#,
        ),
        (
            NumError::base("ParseInt", r#""1""#.into(), 1),
            r#"strconv.ParseInt: parsing "1": invalid base 1"#,
        ),
    ];
    for (num, want) in cases {
        let err = TypesError::from(num);
        assert_eq!(err.0, TypesErrorEnum::Number(want.into()));
        assert_eq!(err.to_string(), want);
    }
}
