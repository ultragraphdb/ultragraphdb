/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod reference_view_body_view;

use crate::types::reference_registration::{REGISTRATION_SIZE, ReferenceRegistration};
use crate::types::request_term_view::RequestTermView;
use crate::types::term_table_view::TermTableView;
use crate::utils::le::bytes_at;

/// The body of a received Reference frame, read in place; see [`crate::Reference`] for its layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReferenceView<'a> {
    pub(crate) namespace: u64,
    reply: bool,
    registrations: &'a [u8],
    terms: TermTableView<'a>,
}

impl<'a> ReferenceView<'a> {
    /// Whether this is the home's reply.
    /// The namespace of the batch's entities.
    pub fn namespace(&self) -> u64 {
        self.namespace
    }

    /// Whether this is the home's reply.
    pub fn is_reply(&self) -> bool {
        self.reply
    }

    /// The number of registrations, at least one.
    pub fn len(&self) -> usize {
        self.registrations.len() / REGISTRATION_SIZE
    }

    /// Whether the body holds no registration; a checked body holds at least one.
    pub fn is_empty(&self) -> bool {
        self.registrations.is_empty()
    }

    /// Registration `index`, `None` past the last.
    pub fn registration(&self, index: usize) -> Option<ReferenceRegistration> {
        let at = index.checked_mul(REGISTRATION_SIZE)?;
        ReferenceRegistration::decode(&bytes_at(self.registrations, at)?)
    }

    /// The registrations, in order.
    pub fn registrations(&self) -> impl Iterator<Item = ReferenceRegistration> + 'a {
        let view = *self;
        (0..view.len()).filter_map(move |index| view.registration(index))
    }

    /// The IRI registration `index` names the entity by, borrowed from the frame; `None` when its system ID names it
    /// or past the last.
    pub fn iri(&self, index: usize) -> Option<&'a str> {
        match self.terms.get(self.registration(index)?.iri()?)? {
            RequestTermView::Iri(iri) => Some(iri),
            RequestTermView::BlankNode(_)
            | RequestTermView::Literal { .. }
            | RequestTermView::LanguageLiteral { .. }
            | RequestTermView::Triple { .. } => None,
        }
    }

    /// The term table of the IRIs the registrations name.
    pub fn terms(&self) -> TermTableView<'a> {
        self.terms
    }
}
