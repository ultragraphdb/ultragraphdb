/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::types::direction::Direction;
use crate::types::request_term::RequestTerm;

/// A term-table entry of a received request, read in place: its texts borrow the request's text section.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RequestTermView<'a> {
    /// An IRI, its text.
    Iri(&'a str),
    /// A blank node, its label, scoped to the request.
    BlankNode(&'a str),
    /// A literal: its lexical form and its datatype's IRI, `None` for `xsd:string`.
    Literal {
        /// The lexical form.
        lexical: &'a str,
        /// The datatype's IRI, `None` for `xsd:string`.
        datatype: Option<&'a str>,
    },
    /// A language-tagged literal: its lexical form, its language tag and its base direction, if it has one.
    LanguageLiteral {
        /// The lexical form.
        lexical: &'a str,
        /// The language tag.
        language: &'a str,
        /// The base direction, if it has one.
        direction: Option<Direction>,
    },
    /// A triple term: the table indices of its subject, predicate and object, each earlier than its own.
    Triple {
        /// The subject's index in the table.
        subject: u32,
        /// The predicate's index in the table.
        predicate: u32,
        /// The object's index in the table.
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
