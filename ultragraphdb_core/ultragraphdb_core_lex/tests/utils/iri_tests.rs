/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, lex/iri_test.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use serde::Deserialize;
use ultragraphdb_core_lex::{
    ItemType, LexErrorEnum, Lexer, has_u_chars, has_x_chars, iri_ref, is_hex, is_iri_ref_char,
};
use ultragraphdb_test_bdd::{load_golden, testdata_dir};

#[test]
fn test_has_u_chars() {
    let test_inputs = [
        ("u1def", true),
        ("UADEFABCD", true),
        ("uYDQW", false),
        ("Uydvxypqt", false),
        ("abc", false),
        ("uabcdg", true),
    ];
    let mut l = Lexer::new("");
    for (input, exp_result) in test_inputs {
        l.reset(input);
        let r = l.next();
        assert_eq!(has_u_chars(r, &mut l), exp_result, "input {input:?}");
    }
}

#[test]
fn test_has_x_chars() {
    let test_inputs = [
        ("xad", true),
        ("xAD", true),
        ("xYD", false),
        ("xyd", false),
        ("abc", false),
    ];
    let mut l = Lexer::new("");
    for (input, exp_result) in test_inputs {
        l.reset(input);
        let r = l.next();
        assert_eq!(has_x_chars(r, &mut l), exp_result, "input {input:?}");
    }
}

#[test]
fn test_is_hex() {
    let test_inputs = [
        ('x', false),
        ('X', false),
        ('L', false),
        ('A', true),
        ('b', true),
    ];
    for (input, exp_result) in test_inputs {
        assert_eq!(is_hex(input), exp_result, "input {input:?}");
    }
}

#[test]
fn test_is_iri_ref_char() {
    let test_inputs = [
        ("\\u1def", true),
        ("\\UADEFABCD", true),
        ("A", true),
        ("a", true),
        ("<", false),
        (">", false),
        ("{", false),
        ("`", false),
        ("\t", false),
        ("\\abc", false),
    ];
    let mut l = Lexer::new("");
    for (input, exp_result) in test_inputs {
        l.reset(input);
        let r = l.next().expect("non-empty input");
        assert_eq!(is_iri_ref_char(r, &mut l), exp_result, "input {input:?}");
    }
}

#[test]
fn test_has_u_chars_at_eof() {
    let mut l = Lexer::new("");
    let r = l.next();
    assert_eq!(r, None);
    assert!(!has_u_chars(r, &mut l));
    assert!(!has_x_chars(r, &mut l));
}

#[test]
fn test_has_u_chars_short() {
    // Three hex digits where four are needed.
    let mut l = Lexer::new("u12");
    let r = l.next();
    assert!(!has_u_chars(r, &mut l));
}

#[test]
fn test_iri_ref_emits_without_brackets() {
    let mut l = Lexer::new("<http://a.b/c> rest");
    l.next();
    iri_ref(&mut l, ItemType::new(7)).expect("valid IRI");
    assert_eq!(l.pos(), 14);
    assert_eq!(l.start(), 14);
    let it = l.new_iterator();
    let mut it = it;
    assert!(it.next());
    assert_eq!(it.item().typ(), ItemType::new(7));
    assert_eq!(it.item().val(), "http://a.b/c");
    assert_eq!(it.item().column(), 1);
    assert!(!it.next());
}

#[test]
fn test_iri_ref_errors_kinds() {
    let mut l = Lexer::new("<abc");
    l.next();
    let err = iri_ref(&mut l, ItemType::new(5)).unwrap_err();
    assert_eq!(err.0, LexErrorEnum::UnexpectedEndOfIri);

    let mut l = Lexer::new("<a b>");
    l.next();
    let err = iri_ref(&mut l, ItemType::new(5)).unwrap_err();
    assert_eq!(err.0, LexErrorEnum::UnexpectedIriChar(' '));
}

#[derive(Deserialize)]
struct ScanCase {
    input: String,
    emitted: Vec<String>,
    error: String,
    pos: usize,
}

#[test]
fn test_iri_ref_golden() {
    let golden = load_golden::<ScanCase>(&testdata_dir().join("golden/lex_iri_ref.json"));
    assert!(!golden.cases().is_empty());
    for case in golden.cases() {
        let mut l = Lexer::new(&case.input);
        l.next();
        let error = match iri_ref(&mut l, ItemType::new(5)) {
            Ok(()) => String::new(),
            Err(e) => e.to_string(),
        };
        assert_eq!(error, case.error, "input {:?}", case.input);
        assert_eq!(l.pos(), case.pos, "input {:?}", case.input);
        let mut it = l.new_iterator();
        let mut emitted = Vec::new();
        while it.next() {
            emitted.push(it.item().val().to_string());
        }
        assert_eq!(emitted, case.emitted, "input {:?}", case.input);
    }
}
