/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use serde::Deserialize;
use ultragraphdb_core_lex::{LexErrorEnum, Lexer};
use ultragraphdb_test_bdd::{load_golden, testdata_dir};

#[derive(Deserialize)]
struct ScanCase {
    input: String,
    error: String,
    pos: usize,
}

#[test]
fn test_lex_quoted_string_golden() {
    let golden = load_golden::<ScanCase>(&testdata_dir().join("golden/lex_quoted_string.json"));
    assert!(!golden.cases().is_empty());
    for case in golden.cases() {
        let mut l = Lexer::new(&case.input);
        l.next();
        let error = match l.lex_quoted_string() {
            Ok(()) => String::new(),
            Err(e) => e.to_string(),
        };
        assert_eq!(error, case.error, "input {:?}", case.input);
        assert_eq!(l.pos(), case.pos, "input {:?}", case.input);
    }
}

#[test]
fn test_lex_quoted_string_error_kinds() {
    let kind = |input: &str| {
        let mut l = Lexer::new(input);
        l.next();
        l.lex_quoted_string().unwrap_err().0.clone()
    };
    assert_eq!(kind("abc\""), LexErrorEnum::NoOpeningQuote);
    assert_eq!(kind("\"abc"), LexErrorEnum::UnexpectedEndOfInput);
    assert_eq!(kind("\"a\\q\""), LexErrorEnum::InvalidEscapeChar(Some('q')));
    assert_eq!(kind("\"a\\"), LexErrorEnum::InvalidEscapeChar(None));
}

#[test]
fn test_lex_quoted_string_leaves_start() {
    let mut l = Lexer::new("x \"a b\" y");
    l.next();
    l.next();
    l.ignore();
    l.next();
    l.lex_quoted_string().expect("closed string");
    assert_eq!(&l.input()[l.start()..l.pos()], "\"a b\"");
}
