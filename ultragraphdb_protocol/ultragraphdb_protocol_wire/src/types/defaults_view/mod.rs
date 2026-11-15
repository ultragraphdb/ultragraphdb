/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod defaults_view_body_view;

use ultragraphdb_error_protocol::ProtocolError;

use crate::types::command::Command;
use crate::types::request_term_view::RequestTermView;
use crate::types::term_table_view::TermTableView;

/// The body of a received Defaults frame, read in place; see [`crate::Defaults`] for its layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefaultsView<'a> {
    tenant: u64,
    position: u64,
    terms: TermTableView<'a>,
    table_bytes: usize,
}

/// The refusal of an entry out of order: [`ProtocolError::field_value`] (`"default order"`, the entry's `index`).
pub(crate) fn out_of_order(index: u64) -> ProtocolError {
    ProtocolError::field_value(Command::Defaults.number(), "default order", index)
}

/// `first`, a frame's first predicate, as the entry after `before`, the last predicate of the frames before it
/// (`PROTOCOL.md` §13.28): above it by its text's bytes, the order running on across the frames.
///
/// # Errors
///
/// [`out_of_order`] of `index`, the entry's index in the answer.
pub(crate) fn follows(
    before: Option<&str>,
    first: Option<&str>,
    index: u64,
) -> Result<(), ProtocolError> {
    match (before, first) {
        (Some(before), Some(first)) if first.as_bytes() <= before.as_bytes() => {
            Err(out_of_order(index))
        }
        (Some(_) | None, Some(_) | None) => Ok(()),
    }
}

impl<'a> DefaultsView<'a> {
    /// The tenant.
    #[must_use]
    pub fn tenant(&self) -> u64 {
        self.tenant
    }

    /// The log position the records were read at; 0 in a request.
    #[must_use]
    pub fn position(&self) -> u64 {
        self.position
    }

    /// Whether the body is a request: `position` 0.
    #[must_use]
    pub fn is_request(&self) -> bool {
        self.position == 0
    }

    /// The number of predicates this frame holds.
    #[must_use]
    pub fn len(&self) -> usize {
        self.terms.len()
    }

    /// Whether this frame holds no predicate.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.terms.is_empty()
    }

    /// Predicate `index` of this frame, its IRI borrowed from the frame; `None` past the last.
    #[must_use]
    pub fn predicate(&self, index: usize) -> Option<&'a str> {
        match self.terms.get(u32::try_from(index).ok()?)? {
            RequestTermView::Iri(iri) => Some(iri),
            RequestTermView::BlankNode(_)
            | RequestTermView::Literal { .. }
            | RequestTermView::LanguageLiteral { .. }
            | RequestTermView::Triple { .. } => None,
        }
    }

    /// The bytes of this frame's term table, its sections 0 and 1.
    pub(crate) fn table_bytes(&self) -> usize {
        self.table_bytes
    }
}
