/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod read_body_builder;

use crate::types::quad_pattern::QuadPattern;
use crate::types::term_table::TermTable;

/// A data request of stage 5 on a stream of its own, in the transaction it names: a quad pattern, at most `limit` rows
/// (0 for no limit) and the resume key a previous result of the same request ended with, empty to start. It reads at
/// the level, token, deadline and read version its transaction's Begin names; each row names its statement by its
/// quad and its reifier's skolem IRI, never by a statement ID.
///
/// | Command header | Field | Type |
/// |---|---|---|
/// | 0 | `transaction` | u64: the stream number of the transaction's Begin |
/// | 8 | `limit` | u32 |
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
    terms: TermTable,
    pattern: QuadPattern,
    resume: Vec<u8>,
}

impl Read {
    /// A read of `pattern` in `transaction`, naming `terms`, at most `limit` rows from `resume`.
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
            terms,
            pattern,
            resume,
        }
    }

    /// The stream number of the Begin of the transaction the read runs in.
    pub fn transaction(&self) -> u64 {
        self.transaction
    }

    /// The most rows returned; 0 for no limit.
    pub fn limit(&self) -> u32 {
        self.limit
    }

    /// The request's term table.
    pub fn terms(&self) -> &TermTable {
        &self.terms
    }

    /// The quad pattern.
    pub fn pattern(&self) -> &QuadPattern {
        &self.pattern
    }

    /// The resume key: where a previous result of the same request ended; empty to start.
    pub fn resume(&self) -> &[u8] {
        &self.resume
    }
}
