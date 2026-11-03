/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The kind byte of a request term-table entry read in place (`PROTOCOL.md` §4.4), which a refusal naming an entry of
//! the wrong kind reports.

use crate::types::request_term_view::RequestTermView;
use crate::types::term_table::kind;

/// The kind byte of `term`: 1 IRI, 2 blank node, 3 literal, 4 language-tagged literal, 5 triple term.
pub(crate) fn term_kind(term: &RequestTermView<'_>) -> u8 {
    match term {
        RequestTermView::Iri(_) => kind::IRI,
        RequestTermView::BlankNode(_) => kind::BLANK_NODE,
        RequestTermView::Literal { .. } => kind::LITERAL,
        RequestTermView::LanguageLiteral { .. } => kind::LANGUAGE_LITERAL,
        RequestTermView::Triple { .. } => kind::TRIPLE,
    }
}
