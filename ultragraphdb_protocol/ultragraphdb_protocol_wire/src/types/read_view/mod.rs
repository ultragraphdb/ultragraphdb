/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod read_view_body_view;

use crate::types::quad_pattern::QuadPattern;
use crate::types::term_table_view::TermTableView;

/// The body of a received Read frame, read in place; see [`crate::Read`] for its layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadView<'a> {
    transaction: u64,
    limit: u32,
    statement_ids: bool,
    terms: TermTableView<'a>,
    pattern: QuadPattern,
    resume: &'a [u8],
}

impl<'a> ReadView<'a> {
    /// The stream number of the transaction's Begin.
    pub fn transaction(&self) -> u64 {
        self.transaction
    }

    /// Whether each row's statement ID is returned.
    pub fn statement_ids(&self) -> bool {
        self.statement_ids
    }

    /// The most rows returned; 0 for no limit.
    pub fn limit(&self) -> u32 {
        self.limit
    }

    /// The request's term table.
    pub fn terms(&self) -> &TermTableView<'a> {
        &self.terms
    }

    /// The quad pattern.
    pub fn pattern(&self) -> &QuadPattern {
        &self.pattern
    }

    /// The resume key: where a previous result of the same request ended; empty to start.
    pub fn resume(&self) -> &'a [u8] {
        self.resume
    }
}
