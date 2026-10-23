/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, tok/bleve.go. Changed for ultragraphdb: translated to Rust; the analyzer bleve
 * builds from Dgraph's definition, run directly.
 * Copyright (c) 2026. Marvin Hansen.
 */

use std::sync::Arc;

use crate::bleve::{lowercase, unicode_tokenizer, unicodenorm};
use crate::types::analysis_token::AnalysisToken;

/// Dgraph's term and fulltext analyzer (`termAnalyzer`, `fulltextAnalyzer`, defined alike): bleve's unicode
/// tokenizer, which splits on Unicode 8.0.0 word boundaries and keeps the word segments, then `to_lower`, then
/// NFKC.
///
/// ```
/// use ultragraphdb_core_tok::analyze;
///
/// let terms: Vec<_> = analyze(b"Dgraph rocks!").iter().map(|t| t.term().to_vec()).collect();
/// assert_eq!(terms, vec![b"dgraph".to_vec(), b"rocks".to_vec()]);
/// ```
pub fn analyze(input: &[u8]) -> Vec<AnalysisToken> {
    let tokens = unicode_tokenizer::tokenize(input);
    // Go's terms are slices of the input, which the lowercase filter rewrites in place, token by token, before
    // the normalizer runs; a term the normalizer leaves unchanged is still that slice.
    let mut buf = input.to_vec();
    let lowered: Vec<(usize, Result<usize, Vec<u8>>)> = tokens
        .iter()
        .map(|(pos, t)| {
            (
                *pos,
                lowercase::to_lower_deferred_copy(&mut buf[*pos..*pos + t.term().len()]),
            )
        })
        .collect();
    let buf: Arc<[u8]> = buf.into();
    tokens
        .into_iter()
        .zip(lowered)
        .map(|((_, mut t), (pos, lower))| {
            let lower = match lower {
                Ok(len) => {
                    t.set_view(&buf, pos, pos + len);
                    t.term().to_vec()
                }
                Err(copy) => copy,
            };
            let normalized = unicodenorm::nfkc(&lower);
            if normalized != t.term() {
                t.set_term(normalized);
            }
            t
        })
        .collect()
}

/// Dgraph's `uniqueTerms`: the terms, sorted and deduplicated.
pub(crate) fn unique_terms(tokens: Vec<AnalysisToken>) -> Vec<Vec<u8>> {
    remove_duplicates(tokens.into_iter().map(|t| t.term().to_vec()).collect())
}

/// Dgraph's `x.RemoveDuplicates`: sorted bytewise, duplicates dropped.
pub(crate) fn remove_duplicates(mut s: Vec<Vec<u8>>) -> Vec<Vec<u8>> {
    s.sort();
    s.dedup();
    s
}
