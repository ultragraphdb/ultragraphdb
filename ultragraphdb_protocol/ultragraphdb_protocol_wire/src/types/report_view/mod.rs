/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod report_view_body_view;

use crate::types::report_kind::ReportKind;
use crate::types::request_term_view::RequestTermView;
use crate::types::term_table_view::TermTableView;

/// The body of a received Report frame, read in place; see [`crate::Report`] for its layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReportView<'a> {
    partition: u32,
    kind: ReportKind,
    tenant: u64,
    version: u64,
    terms: TermTableView<'a>,
}

impl<'a> ReportView<'a> {
    /// The reporting partition.
    pub fn partition(&self) -> u32 {
        self.partition
    }

    /// What the report says.
    pub fn kind(&self) -> ReportKind {
        self.kind
    }

    /// The tenant.
    pub fn tenant(&self) -> u64 {
        self.tenant
    }

    /// The catalog version reported on.
    pub fn version(&self) -> u64 {
        self.version
    }

    /// The predicate's IRI, borrowed from the frame; `None` for a passed check.
    pub fn predicate(&self) -> Option<&'a str> {
        match self.terms.get(0)? {
            RequestTermView::Iri(iri) => Some(iri),
            RequestTermView::BlankNode(_)
            | RequestTermView::Literal { .. }
            | RequestTermView::LanguageLiteral { .. }
            | RequestTermView::Triple { .. } => None,
        }
    }

    /// The subject or object at fault of a refusal, the predicate's IRI when the table names one term; `None` for the
    /// other kinds.
    pub fn at_fault(&self) -> Option<RequestTermView<'a>> {
        match self.kind {
            ReportKind::Refused => self.terms.get(1).or_else(|| self.terms.get(0)),
            ReportKind::Passed | ReportKind::BuildFailed => None,
        }
    }

    /// The term table.
    pub fn terms(&self) -> TermTableView<'a> {
        self.terms
    }
}
