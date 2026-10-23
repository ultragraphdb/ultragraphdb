/*
 * Copyright (c) 2014 Couchbase, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from bleve v2.5.7, analysis/tokenizer/unicode/unicode.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::bleve::segment_words::word_segments;
use crate::types::analysis_token::AnalysisToken;
use crate::types::segment_type::SegmentType;
use crate::types::token_type::TokenType;

/// bleve's `UnicodeTokenizer.Tokenize`: every word segment but the `None` ones, typed. A token's offsets count the
/// bytes of the segments before it, as bleve counts them.
pub(crate) fn tokenize(input: &str) -> Vec<AnalysisToken> {
    let (segments, _) = word_segments(input);
    let mut start = 0;
    let mut out = Vec::new();
    for (text, typ) in segments {
        let end = start + text.len();
        if typ != SegmentType::None {
            out.push(AnalysisToken::new(text, start, end, convert_type(typ)));
        }
        start = end;
    }
    out
}

fn convert_type(t: SegmentType) -> TokenType {
    match t {
        SegmentType::Ideo | SegmentType::Kana => TokenType::Ideographic,
        SegmentType::Number => TokenType::Numeric,
        _ => TokenType::AlphaNumeric,
    }
}
