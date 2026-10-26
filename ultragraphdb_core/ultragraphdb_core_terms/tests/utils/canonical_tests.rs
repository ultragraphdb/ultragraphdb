/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::Literal;
use oxrdf::vocab::xsd;
use ultragraphdb_core_terms::{EncodedTerm, TermError, TermView};

/// The canonical test is the round trip: only a lexical form the value formats back to is encoded by value.
#[test]
fn test_only_a_lexical_form_that_round_trips_is_by_value() {
    let by_value = |l: &str| {
        let literal = Literal::new_typed_literal(l, xsd::INTEGER);
        let term = EncodedTerm::literal(literal.as_ref(), |_| Ok::<_, TermError>(0))
            .unwrap()
            .into_term();
        matches!(term.view().unwrap(), TermView::Integer(_))
    };
    assert!(by_value("42") && by_value("-42") && by_value("0"));
    assert!(!by_value("+42") && !by_value("042") && !by_value("-0") && !by_value("forty-two"));
    assert!(!by_value("99999999999999999999"), "outside 64 bits");
}
