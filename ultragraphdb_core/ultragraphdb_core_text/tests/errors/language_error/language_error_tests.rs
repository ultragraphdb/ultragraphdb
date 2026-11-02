/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_text::{LanguageErrorEnum, Tag};
use ultragraphdb_tool_bdd::{Context, TestResult};

#[test]
fn syntax_error_has_go_text() -> TestResult {
    let err = Tag::parse("x!").err().context("expected an error")?;
    assert_eq!(err.0, LanguageErrorEnum::Syntax);
    assert_eq!(err.to_string(), "language: tag is not well-formed");
    Ok(())
}

#[test]
fn value_error_names_the_subtag() -> TestResult {
    let err = Tag::parse("aa-Uuuu").err().context("expected an error")?;
    assert_eq!(err.0, LanguageErrorEnum::Value(b"Uuuu".to_vec()));
    assert_eq!(
        err.to_string(),
        "language: subtag \"Uuuu\" is well-formed but unknown"
    );
    Ok(())
}

#[test]
fn unknown_m49_code_has_an_empty_subtag() -> TestResult {
    let err = Tag::parse("en-000").err().context("expected an error")?;
    assert_eq!(
        err.to_string(),
        "language: subtag \"\" is well-formed but unknown"
    );
    Ok(())
}

#[test]
fn duplicate_key_error_has_go_text() -> TestResult {
    let err = Tag::parse("en-u-co-phonebk-co-pinyin")
        .err()
        .context("expected an error")?;
    assert_eq!(err.0, LanguageErrorEnum::DuplicateKey);
    assert_eq!(
        err.to_string(),
        "language: different values for same key in -u extension"
    );
    Ok(())
}
