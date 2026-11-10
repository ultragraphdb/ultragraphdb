/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod purge_view_body_view;

use crate::types::request_term_view::RequestTermView;
use crate::types::term_table_view::TermTableView;

/// The body of a received Purge frame, read in place; see [`crate::Purge`] for its layout. A body of `epoch` 0 is a
/// request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PurgeView<'a> {
    tenant: u64,
    epoch: u64,
    removed: u32,
    terms: TermTableView<'a>,
}

impl<'a> PurgeView<'a> {
    /// The tenant.
    #[must_use]
    pub fn tenant(&self) -> u64 {
        self.tenant
    }

    /// In a reply, the purge epoch; 0 in a request.
    #[must_use]
    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    /// In a reply, the instances removed; 0 in a request.
    #[must_use]
    pub fn removed(&self) -> u32 {
        self.removed
    }

    /// The number of IRIs.
    #[must_use]
    pub fn iri_count(&self) -> usize {
        self.terms.len()
    }

    /// IRI `index`, borrowed from the frame; `None` past the last.
    #[must_use]
    pub fn iri(&self, index: usize) -> Option<&'a str> {
        match self.terms.get(u32::try_from(index).ok()?)? {
            RequestTermView::Iri(iri) => Some(iri),
            RequestTermView::BlankNode(_)
            | RequestTermView::Literal { .. }
            | RequestTermView::LanguageLiteral { .. }
            | RequestTermView::Triple { .. } => None,
        }
    }
}
