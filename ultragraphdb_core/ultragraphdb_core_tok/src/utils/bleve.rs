/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, tok/bleve.go. Changed for ultragraphdb: translated to Rust; the analyzer bleve
 * builds from Dgraph's definition, run directly.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::bleve::{lowercase, unicode_tokenizer, unicodenorm};
use crate::types::analysis_token::AnalysisToken;

/// Dgraph's term and fulltext analyzer (`termAnalyzer`, `fulltextAnalyzer`, defined alike): bleve's unicode
/// tokenizer, which splits on Unicode 8.0.0 word boundaries and keeps the word segments, then `to_lower`, then
/// NFKC.
///
/// ```
/// use ultragraphdb_core_tok::analyze;
///
/// let terms: Vec<_> = analyze("Dgraph rocks!").iter().map(|t| t.term().to_string()).collect();
/// assert_eq!(terms, vec!["dgraph", "rocks"]);
/// ```
pub fn analyze(input: &str) -> Vec<AnalysisToken> {
    unicode_tokenizer::tokenize(input)
        .into_iter()
        .map(|mut t| {
            let term = unicodenorm::nfkc(&lowercase::to_lower(t.term()));
            t.set_term(term);
            t
        })
        .collect()
}

/// Dgraph's `uniqueTerms`: the terms, sorted and deduplicated.
pub(crate) fn unique_terms(tokens: Vec<AnalysisToken>) -> Vec<Vec<u8>> {
    remove_duplicates(
        tokens
            .into_iter()
            .map(|t| t.term().as_bytes().to_vec())
            .collect(),
    )
}

/// Dgraph's `x.RemoveDuplicates`: sorted bytewise, duplicates dropped.
pub(crate) fn remove_duplicates(mut s: Vec<Vec<u8>>) -> Vec<Vec<u8>> {
    s.sort();
    s.dedup();
    s
}
