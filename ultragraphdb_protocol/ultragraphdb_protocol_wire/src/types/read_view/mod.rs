/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod read_view_body_view;

use xxhash_rust::xxh3::xxh3_64;

use crate::types::quad_pattern::QuadPattern;
use crate::types::term_table_view::TermTableView;

/// The body of a received Read frame, read in place; see [`crate::Read`] for its layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadView<'a> {
    transaction: u64,
    limit: u32,
    terms: TermTableView<'a>,
    pattern: QuadPattern,
    resume: &'a [u8],
    /// Sections 0 to 2, the pattern, the records and the text, which the layout places with no byte between them.
    pattern_sections: &'a [u8],
}

impl<'a> ReadView<'a> {
    /// The stream number of the transaction's Begin.
    #[must_use]
    pub fn transaction(&self) -> u64 {
        self.transaction
    }

    /// The most rows returned; 0 for no limit.
    #[must_use]
    pub fn limit(&self) -> u32 {
        self.limit
    }

    /// The request's term table.
    #[must_use]
    pub fn terms(&self) -> &TermTableView<'a> {
        &self.terms
    }

    /// The quad pattern.
    #[must_use]
    pub fn pattern(&self) -> &QuadPattern {
        &self.pattern
    }

    /// The resume key: where a previous result of the same request ended; empty to start.
    #[must_use]
    pub fn resume(&self) -> &'a [u8] {
        self.resume
    }

    /// The pattern's hash a resume key holds (`PROTOCOL.md` §11.8): XXH3-64, seed 0, of the body's sections 0 to 2 read in
    /// place, the pattern, the term table's records and its text, which the body's layout places with no byte between them.
    #[must_use]
    pub fn pattern_hash(&self) -> u64 {
        xxh3_64(self.pattern_sections)
    }
}
