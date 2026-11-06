/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_protocol_wire::{RequestTerm, RequestTermView, ResumeKeyView};

/// A resume key's last row by its terms' text, as the key holds them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResumeTerms {
    pub(crate) graph: Option<String>,
    pub(crate) subject: String,
    pub(crate) predicate: String,
    pub(crate) object: RequestTerm,
    pub(crate) reifier: Option<String>,
}

/// The IRI of `index` of the key's table; `None` for any other term.
fn iri_at(key: &ResumeKeyView<'_>, index: u32) -> Option<String> {
    match key.terms().get(index) {
        Some(RequestTermView::Iri(iri)) => Some(iri.to_string()),
        Some(
            RequestTermView::BlankNode(_)
            | RequestTermView::Literal { .. }
            | RequestTermView::LanguageLiteral { .. }
            | RequestTermView::Triple { .. },
        )
        | None => None,
    }
}

impl ResumeTerms {
    /// The row `key` holds; `None` when a position holds a term of a kind no row names.
    pub(crate) fn of(key: &ResumeKeyView<'_>) -> Option<Self> {
        let row = key.row();
        let graph = match row.graph() {
            Some(index) => Some(iri_at(key, index)?),
            None => None,
        };
        let reifier = match row.reifier() {
            Some(index) => Some(iri_at(key, index)?),
            None => None,
        };
        Some(Self {
            graph,
            subject: iri_at(key, row.subject())?,
            predicate: iri_at(key, row.predicate())?,
            object: key.terms().get(row.object())?.to_term(),
            reifier,
        })
    }
}
