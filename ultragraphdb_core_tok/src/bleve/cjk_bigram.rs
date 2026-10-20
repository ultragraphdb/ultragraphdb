/*
 * Copyright (c) 2014 Couchbase, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from bleve v2.5.7, analysis/lang/cjk/cjk_bigram.go, with `output_unigram` off as Dgraph's
 * `cjk_bigram` filter has it. Changed for ultragraphdb: translated to Rust; Go's two-element container/ring as two
 * slots and a cursor.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::bleve::go_utf8::runes;
use crate::types::analysis_token::AnalysisToken;
use crate::types::token_type::TokenType;

/// The ring of two tokens bleve buffers ideographic runes in.
struct Ring {
    slots: [Option<AnalysisToken>; 2],
    cur: usize,
    items: usize,
}

impl Ring {
    fn value(&self) -> &AnalysisToken {
        self.slots[self.cur].as_ref().expect("ring value")
    }

    fn prev(&self) -> &AnalysisToken {
        self.slots[self.cur ^ 1].as_ref().expect("ring value")
    }

    /// bleve's `flush`: the buffered rune as a unigram when it is alone, and the ring emptied.
    fn flush(&mut self) -> Option<AnalysisToken> {
        let rv = (self.items == 1).then(|| self.unigram());
        self.slots[self.cur] = None;
        self.items = 0;
        rv
    }

    /// bleve's `buildUnigram` for one buffered token.
    fn unigram(&self) -> AnalysisToken {
        let prev = self.value();
        AnalysisToken::new(prev.term(), prev.start(), prev.end(), TokenType::Single)
    }

    /// bleve's `outputBigram`.
    fn bigram(&self) -> Option<AnalysisToken> {
        (self.items == 2).then(|| {
            let (prev, curr) = (self.prev(), self.value());
            let mut term = prev.term().to_vec();
            term.extend_from_slice(curr.term());
            AnalysisToken::new(term, prev.start(), curr.end(), TokenType::Double)
        })
    }
}

/// bleve's `CJKBigramFilter.Filter`: the runes of adjoining ideographic tokens as overlapping bigrams, a lone
/// ideographic rune as a unigram, and every other token as it is.
pub(crate) fn filter(input: Vec<AnalysisToken>) -> Vec<AnalysisToken> {
    let mut r = Ring {
        slots: [None, None],
        cur: 0,
        items: 0,
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
        // bleve slices the term by the lengths of the runes it decoded, an invalid byte counting as the three bytes
        // of U+FFFD, which reads past the term into its buffer; Go panics past the buffer's end, this port stops.
        let mut term = tok.term().to_vec();
        term.extend_from_slice(tok.tail());
        let mut sofar = 0;
        for run in runes(tok.term()) {
            let rlen = run.len_utf8();
            let (from, to) = (sofar.min(term.len()), (sofar + rlen).min(term.len()));
            let token = AnalysisToken::new(
                &term[from..to],
                tok.start() + sofar,
                tok.start() + sofar + rlen,
                tok.typ(),
            );
            sofar += rlen;
            if r.items > 0
                && token.start() != r.value().end()
                && let Some(t) = r.flush()
            {
                rv.push(t);
            }
            r.cur ^= 1;
            r.slots[r.cur] = Some(token);
            if r.items < 2 {
                r.items += 1;
            }
            if let Some(t) = r.bigram() {
                rv.push(t);
            }
        }
    }
    if r.items == 1 {
        rv.push(r.unigram());
    }
    rv
}
