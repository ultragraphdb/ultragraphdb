/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::types::direction::Direction;
use crate::types::request_term::RequestTerm;

/// A term-table entry of a received request, read in place: its texts borrow the request's text section.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RequestTermView<'a> {
    Iri(&'a str),
    BlankNode(&'a str),
    Literal {
        lexical: &'a str,
        datatype: Option<&'a str>,
    },
    LanguageLiteral {
        lexical: &'a str,
        language: &'a str,
        direction: Option<Direction>,
    },
    Triple {
        subject: u32,
        predicate: u32,
        object: u32,
    },
}

impl RequestTermView<'_> {
    /// The owned term.
    pub fn to_term(&self) -> RequestTerm {
        match *self {
            RequestTermView::Iri(text) => RequestTerm::Iri(text.to_string()),
            RequestTermView::BlankNode(label) => RequestTerm::BlankNode(label.to_string()),
            RequestTermView::Literal { lexical, datatype } => RequestTerm::Literal {
                lexical: lexical.to_string(),
                datatype: datatype.map(str::to_string),
            },
            RequestTermView::LanguageLiteral {
                lexical,
                language,
                direction,
            } => RequestTerm::LanguageLiteral {
                lexical: lexical.to_string(),
                language: language.to_string(),
                direction,
            },
            RequestTermView::Triple {
                subject,
                predicate,
                object,
            } => RequestTerm::Triple {
                subject,
                predicate,
                object,
            },
        }
    }
}
