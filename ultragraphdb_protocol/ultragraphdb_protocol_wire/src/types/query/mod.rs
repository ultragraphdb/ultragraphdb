/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

mod query_body_builder;

use crate::types::language::Language;
use crate::types::term_table::TermTable;

/// A query of the GQL core or the SPARQL core in the transaction it names, on a stream of its own, as a
/// [`crate::Read`] is (`PROTOCOL.md` §7.4): its text, its parameters by name, at most `limit` rows and the resume key a
/// previous result of the same request ended with. Answered with Result frames, the first carrying the column names.
///
/// | Command header | Field | Type |
/// |---|---|---|
/// | 0 | `transaction` | u64: the stream number of the transaction's Begin |
/// | 8 | `language` | u8, a [`Language`]: 1 GQL, 2 SPARQL |
/// | 12 | `limit` | u32 |
///
/// | Section | Contents |
/// |---|---|
/// | 0 | the query text, UTF-8, at most the receiver's `max_frame` less [`crate::QUERY_TEXT_RESERVE`] bytes |
/// | 1 | parameter names: ends |
/// | 2 | parameter names: text, each without its `$` or `?` |
/// | 3 | the parameters' values, u32 term indices, one per name |
/// | 4 | the term table's records |
/// | 5 | the term table's text |
/// | 6 | the resume key |
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Query {
    transaction: u64,
    language: Language,
    limit: u32,
    text: String,
    parameters: Vec<(String, u32)>,
    terms: TermTable,
    resume: Vec<u8>,
}

impl Query {
    /// The query `text` in `language`, in `transaction`, naming `terms`, of no limit and no parameter, from its start.
    pub fn new(
        transaction: u64,
        language: Language,
        text: impl Into<String>,
        terms: TermTable,
    ) -> Self {
        Self {
            transaction,
            language,
            limit: 0,
            text: text.into(),
            parameters: Vec::new(),
            terms,
            resume: Vec::new(),
        }
    }

    /// The query of at most `limit` rows; 0 for no limit.
    #[must_use]
    pub fn with_limit(mut self, limit: u32) -> Self {
        self.limit = limit;
        self
    }

    /// The query with one more parameter: `name`, without its `$` or `?`, bound to the term at index `value`.
    #[must_use]
    pub fn with_parameter(mut self, name: impl Into<String>, value: u32) -> Self {
        self.parameters.push((name.into(), value));
        self
    }

    /// The query continuing from the resume key `resume`.
    #[must_use]
    pub fn with_resume(mut self, resume: Vec<u8>) -> Self {
        self.resume = resume;
        self
    }

    /// The stream number of the Begin of the transaction the query runs in.
    #[must_use]
    pub fn transaction(&self) -> u64 {
        self.transaction
    }

    /// The language of its text.
    #[must_use]
    pub fn language(&self) -> Language {
        self.language
    }

    /// The most rows returned; 0 for no limit.
    #[must_use]
    pub fn limit(&self) -> u32 {
        self.limit
    }

    /// The query text.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The parameters, each a name and its term's index, in order.
    #[must_use]
    pub fn parameters(&self) -> &[(String, u32)] {
        &self.parameters
    }

    /// The request's term table.
    #[must_use]
    pub fn terms(&self) -> &TermTable {
        &self.terms
    }

    /// The resume key: where a previous result of the same request ended; empty to start.
    #[must_use]
    pub fn resume(&self) -> &[u8] {
        &self.resume
    }
}
