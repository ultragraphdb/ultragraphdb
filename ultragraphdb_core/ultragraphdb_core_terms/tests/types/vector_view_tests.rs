/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::{Literal, NamedNode};
use ultragraphdb_core_terms::{EncodedTerm, FLOAT32_VECTOR, TermError, TermView};

#[test]
fn test_a_view_reads_the_elements_in_place() {
    let literal =
        Literal::new_typed_literal("[1, -2.5, 3]", NamedNode::new_unchecked(FLOAT32_VECTOR));
    let term = EncodedTerm::literal(literal.as_ref(), |_| Ok::<_, TermError>(0))
        .unwrap()
        .into_term();
    let TermView::Vector(view) = term.view().unwrap() else {
        panic!("a vector");
    };
    assert_eq!((view.len(), view.is_empty()), (3, false));
    assert_eq!(view.iter().collect::<Vec<_>>(), vec![1.0, -2.5, 3.0]);
    assert_eq!(view.as_bytes(), &term.as_bytes()[3..]);
}
