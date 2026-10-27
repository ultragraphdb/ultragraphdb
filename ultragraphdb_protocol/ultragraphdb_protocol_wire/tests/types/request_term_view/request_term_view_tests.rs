/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{Direction, RequestTerm, RequestTermView};

#[test]
fn test_a_view_owns_its_term() {
    assert_eq!(
        RequestTermView::Iri("a").to_term(),
        RequestTerm::Iri("a".to_string())
    );
    assert_eq!(
        RequestTermView::BlankNode("b").to_term(),
        RequestTerm::BlankNode("b".to_string())
    );
    assert_eq!(
        RequestTermView::Literal {
            lexical: "1",
            datatype: Some("d")
        }
        .to_term(),
        RequestTerm::Literal {
            lexical: "1".to_string(),
            datatype: Some("d".to_string())
        }
    );
    assert_eq!(
        RequestTermView::Literal {
            lexical: "1",
            datatype: None
        }
        .to_term(),
        RequestTerm::Literal {
            lexical: "1".to_string(),
            datatype: None
        }
    );
    assert_eq!(
        RequestTermView::LanguageLiteral {
            lexical: "x",
            language: "en",
            direction: Some(Direction::Ltr)
        }
        .to_term(),
        RequestTerm::LanguageLiteral {
            lexical: "x".to_string(),
            language: "en".to_string(),
            direction: Some(Direction::Ltr)
        }
    );
    assert_eq!(
        RequestTermView::Triple {
            subject: 1,
            predicate: 2,
            object: 3
        }
        .to_term(),
        RequestTerm::Triple {
            subject: 1,
            predicate: 2,
            object: 3
        }
    );
}
