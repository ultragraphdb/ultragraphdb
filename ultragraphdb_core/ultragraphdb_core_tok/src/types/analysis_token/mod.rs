/*
 * Copyright (c) 2014 Couchbase, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from bleve v2.5.7, analysis/type.go (Token). Changed for ultragraphdb: translated to Rust; the fields
 * Dgraph's analysis reads.
 * Copyright (c) 2026. Marvin Hansen.
 */

mod analysis_token_part_eq;

use crate::types::token_type::TokenType;

/// One token of a bleve token stream: its term and the byte offsets the tokenizer gave it in the input. Filters
/// change the term and keep the offsets, which the CJK bigram filter reads to see whether two runes adjoin.
#[derive(Debug, Clone)]
pub struct AnalysisToken {
    term: String,
    start: usize,
    end: usize,
    typ: TokenType,
}

impl AnalysisToken {
    /// A token of `term` at `start..end` of the input.
    pub fn new(term: impl Into<String>, start: usize, end: usize, typ: TokenType) -> Self {
        Self {
            term: term.into(),
            start,
            end,
            typ,
        }
    }

    /// An alphanumeric token of `term` at offset 0, as Dgraph's filter tests build them.
    pub fn of(term: impl Into<String>) -> Self {
        Self::new(term, 0, 0, TokenType::AlphaNumeric)
    }

    /// The term.
    pub fn term(&self) -> &str {
        &self.term
    }

    /// The first byte of the token in the input.
    pub fn start(&self) -> usize {
        self.start
    }

    /// The byte after the token in the input.
    pub fn end(&self) -> usize {
        self.end
    }

    /// The token's type.
    pub fn typ(&self) -> TokenType {
        self.typ
    }

    /// The token with another term.
    pub(crate) fn set_term(&mut self, term: String) {
        self.term = term;
    }
}
