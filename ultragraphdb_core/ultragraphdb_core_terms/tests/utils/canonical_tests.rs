/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::Literal;
use oxrdf::vocab::xsd;
use ultragraphdb_core_terms::{EncodedTerm, TermError, TermView};

/// The value an `xsd:integer` lexical form is held as, when it is held by value.
fn by_value(lexical: &str) -> Option<i64> {
    let literal = Literal::new_typed_literal(lexical, xsd::INTEGER);
    let term = EncodedTerm::literal(literal.as_ref(), |_| Ok::<_, TermError>(0))
        .unwrap()
        .into_term();
    match term.view().unwrap() {
        TermView::Integer(v) => Some(i64::from(v)),
        _ => None,
    }
}

/// The canonical test is the round trip: only a lexical form the value formats back to is encoded by value.
#[test]
fn test_only_a_lexical_form_that_round_trips_is_by_value() {
    // The values each decimal numeral names, and i64's bounds, -2^63 and 2^63 - 1.
    for (lexical, value) in [
        ("42", 42),
        ("-42", -42),
        ("0", 0),
        ("9223372036854775807", i64::MAX),
        ("-9223372036854775808", i64::MIN),
    ] {
        assert_eq!(by_value(lexical), Some(value), "{lexical}");
    }
    for lexical in ["+42", "042", "-0", "forty-two", "", " 42", "42 "] {
        assert_eq!(by_value(lexical), None, "{lexical:?}");
    }
    assert_eq!(by_value("99999999999999999999"), None, "outside 64 bits");
    assert_eq!(by_value("9223372036854775808"), None, "2^63, one past i64");
}
