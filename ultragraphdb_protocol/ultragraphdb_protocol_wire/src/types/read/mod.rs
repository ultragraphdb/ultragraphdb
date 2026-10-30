/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod read_body_builder;

use crate::types::quad_pattern::QuadPattern;
use crate::types::term_table::TermTable;

/// A data request of stage 5 on a stream of its own, in the transaction it names: a quad pattern, at most `limit` rows
/// (0 for no limit), the resume key a previous result of the same request ended with, empty to start, and whether
/// each row's statement ID is returned.
///
/// | Command header | Field | Type |
/// |---|---|---|
/// | 0 | `transaction` | u64: the stream number of the transaction's Begin |
/// | 8 | `limit` | u32 |
/// | 12 | `flags` | u32: bit 0, return each row's statement ID; bits 1 to 31 zero |
///
/// | Section | Contents |
/// |---|---|
/// | 0 | the pattern: four [`crate::PatternSlot`]s, no parameter |
/// | 1 | the term table's records |
/// | 2 | the term table's text |
/// | 3 | the resume key |
///
/// The request's terms are a [`crate::TermTable`] in two sections, its records and its text; every term the body
/// names is an index into it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Read {
    transaction: u64,
    limit: u32,
    statement_ids: bool,
    terms: TermTable,
    pattern: QuadPattern,
    resume: Vec<u8>,
}

impl Read {
    pub fn new(
        transaction: u64,
        limit: u32,
        terms: TermTable,
        pattern: QuadPattern,
        resume: Vec<u8>,
    ) -> Self {
        Self {
            transaction,
            limit,
            statement_ids: false,
            terms,
            pattern,
            resume,
        }
    }

    /// The read asking for each row's statement ID.
    pub fn with_statement_ids(mut self) -> Self {
        self.statement_ids = true;
        self
    }

    /// The stream number of the Begin of the transaction the read runs in.
    pub fn transaction(&self) -> u64 {
        self.transaction
    }

    /// Whether each row's statement ID is returned.
    pub fn statement_ids(&self) -> bool {
        self.statement_ids
    }

    pub fn limit(&self) -> u32 {
        self.limit
    }

    pub fn terms(&self) -> &TermTable {
        &self.terms
    }

    pub fn pattern(&self) -> &QuadPattern {
        &self.pattern
    }

    pub fn resume(&self) -> &[u8] {
        &self.resume
    }
}
