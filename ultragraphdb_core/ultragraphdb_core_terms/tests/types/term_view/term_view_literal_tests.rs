/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use oxrdf::NamedNode;
use ultragraphdb_core_terms::{Datatype, Hash128, TermError, TermView, Text};

#[test]
fn test_a_uid_is_no_literal() {
    let refused = TermView::Uid(5).to_literal(
        |_| Ok::<_, TermError>(NamedNode::new_unchecked("http://e/x")),
        |_| Ok(String::new()),
    );
    assert_eq!(refused, Err(TermError::NotALiteral(5)));
}

#[test]
fn test_a_resolver_failure_is_returned() {
    #[derive(Debug, PartialEq)]
    struct Missing(u64);
    impl From<TermError> for Missing {
        fn from(_: TermError) -> Self {
            Missing(0)
        }
    }
    let view = TermView::Typed {
        datatype: Datatype::Uid(9),
        lexical: Text::Inline("x"),
    };
    assert_eq!(
        view.to_literal(|uid| Err(Missing(uid)), |_| Ok(String::new())),
        Err(Missing(9))
    );
    let hashed = TermView::String(Text::Hashed(Hash128::of(b"gone")));
    assert_eq!(
        hashed.to_literal(|_| Err(Missing(1)), |_| Err(Missing(2))),
        Err(Missing(2))
    );
}
