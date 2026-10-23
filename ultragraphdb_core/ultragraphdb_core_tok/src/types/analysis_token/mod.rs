/*
 * Copyright (c) 2014 Couchbase, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from bleve v2.5.7, analysis/type.go (Token). Changed for ultragraphdb: translated to Rust; the fields
 * Dgraph's analysis reads.
 * Copyright (c) 2026. Marvin Hansen.
 */

mod analysis_token_part_eq;

use std::sync::Arc;

use crate::types::token_type::TokenType;

/// One token of a bleve token stream: its term and the byte offsets the tokenizer gave it in the input. Filters
/// change the term and keep the offsets, which the CJK bigram filter reads to see whether two runes adjoin.
///
/// A Go term is a slice of a larger buffer, the input as long as no filter copied it, and bleve's CJK bigram
/// filter reads past a term's end into that buffer when the term holds invalid UTF-8; `tail` keeps the buffer and
/// the offset after the term. Equality compares the term, offsets and type only.
#[derive(Debug, Clone)]
pub struct AnalysisToken {
    term: Vec<u8>,
    start: usize,
    end: usize,
    typ: TokenType,
    tail: Option<(Arc<[u8]>, usize)>,
}

impl AnalysisToken {
    /// A token of `term` at `start..end` of the input.
    pub fn new(term: impl Into<Vec<u8>>, start: usize, end: usize, typ: TokenType) -> Self {
        Self {
            term: term.into(),
            start,
            end,
            typ,
            tail: None,
        }
    }

    /// An alphanumeric token of `term` at offset 0, as Dgraph's filter tests build them.
    pub fn of(term: impl Into<Vec<u8>>) -> Self {
        Self::new(term, 0, 0, TokenType::AlphaNumeric)
    }

    /// The term.
    pub fn term(&self) -> &[u8] {
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

    /// The token with another term, a copy: nothing follows it in its buffer.
    pub(crate) fn set_term(&mut self, term: Vec<u8>) {
        self.term = term;
        self.tail = None;
    }

    /// The token with the term `buffer[from..to]`, still a slice of `buffer`.
    pub(crate) fn set_view(&mut self, buffer: &Arc<[u8]>, from: usize, to: usize) {
        self.term = buffer[from..to].to_vec();
        self.tail = Some((Arc::clone(buffer), to));
    }

    /// The bytes after the term in its buffer.
    pub(crate) fn tail(&self) -> &[u8] {
        self.tail.as_ref().map_or(&[], |(b, off)| &b[*off..])
    }
}
