/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_lex::{ItemType, LexError, LexErrorEnum, Lexer, iri_ref};

fn quoted_string_error(input: &str) -> LexError {
    let mut l = Lexer::new(input);
    l.next();
    l.lex_quoted_string().unwrap_err()
}

#[test]
fn test_kind_classifies_the_failure() {
    assert_eq!(quoted_string_error("x").0, LexErrorEnum::NoOpeningQuote);
    assert_eq!(
        quoted_string_error("\"").0,
        LexErrorEnum::UnexpectedEndOfInput
    );

    let mut l = Lexer::new("<");
    l.next();
    assert_eq!(
        iri_ref(&mut l, ItemType::new(5)).unwrap_err().0,
        LexErrorEnum::UnexpectedEndOfIri
    );

    let l = Lexer::new("");
    assert_eq!(
        l.new_iterator().peek(1).unwrap_err().0,
        LexErrorEnum::PeekOutOfRange
    );
}

#[test]
fn test_errors_compare_and_clone() {
    let a = quoted_string_error("\"a\\z\"");
    let b = a.clone();
    assert_eq!(a, b);
    assert_ne!(a, quoted_string_error("x"));
}
