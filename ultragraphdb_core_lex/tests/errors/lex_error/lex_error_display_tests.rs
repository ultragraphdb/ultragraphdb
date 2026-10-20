/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_lex::{ItemType, LexError, Lexer, iri_ref};

fn quoted_string_error(input: &str) -> LexError {
    let mut l = Lexer::new(input);
    l.next();
    l.lex_quoted_string().unwrap_err()
}

fn iri_error(input: &str) -> LexError {
    let mut l = Lexer::new(input);
    l.next();
    iri_ref(&mut l, ItemType::new(5)).unwrap_err()
}

#[test]
fn test_display_matches_dgraph() {
    assert_eq!(
        quoted_string_error("x").to_string(),
        "String should start with quote."
    );
    assert_eq!(
        quoted_string_error("\"").to_string(),
        "Unexpected end of input."
    );
    assert_eq!(
        quoted_string_error("\"\\x\"").to_string(),
        "Not a valid escape char: 'x'"
    );
    // Go prints EOF (-1) with %c as the replacement character.
    assert_eq!(
        quoted_string_error("\"\\").to_string(),
        "Not a valid escape char: '\u{FFFD}'"
    );
    assert_eq!(iri_error("<a").to_string(), "Unexpected end of IRI");
    assert_eq!(
        iri_error("<a\"").to_string(),
        "Unexpected character '\"' while parsing IRI"
    );
    assert_eq!(
        iri_error("<\u{1}").to_string(),
        "Unexpected character '\\x01' while parsing IRI"
    );
    assert_eq!(
        iri_error("<\n").to_string(),
        "Unexpected character '\\n' while parsing IRI"
    );
    assert_eq!(
        iri_error("<a\\").to_string(),
        "Unexpected character '\\\\' while parsing IRI"
    );
    assert_eq!(
        Lexer::new("")
            .new_iterator()
            .peek(1)
            .unwrap_err()
            .to_string(),
        "Out of range for peek"
    );
}
