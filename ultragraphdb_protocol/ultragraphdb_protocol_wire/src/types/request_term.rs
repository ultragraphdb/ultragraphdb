/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::types::direction::Direction;

/// An RDF term a request names, by text: the owned form of a term-table entry. A triple term names its subject,
/// predicate and object by the indices of earlier entries of the same table.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RequestTerm {
    /// An IRI, its text.
    Iri(String),
    /// A blank node, its label, scoped to the request.
    BlankNode(String),
    /// A literal: its lexical form and its datatype's IRI, `None` for `xsd:string`.
    Literal {
        lexical: String,
        datatype: Option<String>,
    },
    /// A language-tagged literal: its lexical form, its language tag and its base direction, if it has one.
    LanguageLiteral {
        lexical: String,
        language: String,
        direction: Option<Direction>,
    },
    /// A triple term: the table indices of its subject, predicate and object, each earlier than its own.
    Triple {
        subject: u32,
        predicate: u32,
        object: u32,
    },
}
