/*
 * Copyright (c) 2014 Couchbase, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from bleve v2.5.7, analysis/lang/cjk/cjk_bigram.go, with `output_unigram` off as Dgraph's
 * `cjk_bigram` filter has it. Changed for ultragraphdb: translated to Rust; Go's two-element container/ring as two
 * slots and a cursor.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::types::analysis_token::AnalysisToken;
use crate::types::token_type::TokenType;

/// The ring of two tokens bleve buffers ideographic runes in: the token pushed last and, when two were pushed since
/// the last flush, the one before it.
struct Ring {
    previous: Option<AnalysisToken>,
    recent: Option<AnalysisToken>,
}

impl Ring {
    /// Buffers `token` as the most recent, the one it displaces as the previous.
    fn push(&mut self, token: AnalysisToken) {
        self.previous = self.recent.replace(token);
    }

    /// bleve's `flush`: the buffered rune as a unigram when it is alone, and the ring emptied.
    fn flush(&mut self) -> Option<AnalysisToken> {
        match (self.previous.take(), self.recent.take()) {
            (None, Some(alone)) => Some(unigram(&alone)),
            _ => None,
        }
    }

    /// bleve's `outputBigram`.
    fn bigram(&self) -> Option<AnalysisToken> {
        let (Some(prev), Some(curr)) = (&self.previous, &self.recent) else {
            return None;
        };
        let term = format!("{}{}", prev.term(), curr.term());
        Some(AnalysisToken::new(
            term,
            prev.start(),
            curr.end(),
            TokenType::Double,
        ))
    }
}

/// bleve's `buildUnigram` for one buffered token.
fn unigram(prev: &AnalysisToken) -> AnalysisToken {
    AnalysisToken::new(prev.term(), prev.start(), prev.end(), TokenType::Single)
}

/// bleve's `CJKBigramFilter.Filter`: the runes of adjoining ideographic tokens as overlapping bigrams, a lone
/// ideographic rune as a unigram, and every other token as it is.
pub(crate) fn filter(input: Vec<AnalysisToken>) -> Vec<AnalysisToken> {
    let mut r = Ring {
        previous: None,
        recent: None,
    };
    let mut rv = Vec::with_capacity(input.len());
    for tok in input {
        if tok.typ() != TokenType::Ideographic {
            if let Some(t) = r.flush() {
                rv.push(t);
            }
            rv.push(tok);
            continue;
        }
        // bleve slices the term by the widths of its runes, counting an invalid byte as the three bytes of U+FFFD,
        // which reads past the term (ledger 17); a `&str` term holds no invalid byte, and each rune is a token.
        for (sofar, run) in tok.term().char_indices() {
            let start = tok.start() + sofar;
            let token = AnalysisToken::new(run, start, start + run.len_utf8(), tok.typ());
            if r.recent.as_ref().is_some_and(|v| token.start() != v.end())
                && let Some(t) = r.flush()
            {
                rv.push(t);
            }
            r.push(token);
            if let Some(t) = r.bigram() {
                rv.push(t);
            }
        }
    }
    if let Some(t) = r.flush() {
        rv.push(t);
    }
    rv
}
