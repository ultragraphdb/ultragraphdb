/*
 * Copyright (c) 2014 Couchbase, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from bleve v2.5.7, analysis/token/unicodenorm/unicodenorm_test.go (TestUnicodeNormalization). Changed for
 * ultragraphdb: translated to Rust; the NFKC cases, the one form Dgraph defines, reached through `analyze`, and
 * further compatibility characters with the terms bleve's analyzer gives them, and every scalar's analysis
 * hashed against Go's.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_core_tok::{AnalysisToken, TokenType, analyze};

#[test]
fn test_unicode_normalization_nfkc() {
    let tests: [(&str, Vec<AnalysisToken>); 9] = [
        // bleve's NFKC cases: halfwidth katakana compose, a ligature splits.
        (
            "ｳﾞｨｯﾂ",
            vec![AnalysisToken::new(
                "ヴィッツ",
                0,
                15,
                TokenType::Ideographic,
            )],
        ),
        (
            "ﾊﾟﾅｿﾆｯｸ",
            vec![AnalysisToken::new(
                "パナソニック",
                0,
                21,
                TokenType::Ideographic,
            )],
        ),
        (
            "\u{FB01}",
            vec![AnalysisToken::new(
                "\u{0066}\u{0069}",
                0,
                3,
                TokenType::AlphaNumeric,
            )],
        ),
        // The offsets stay those of the input.
        (
            "ﬁnancial",
            vec![AnalysisToken::new(
                "financial",
                0,
                10,
                TokenType::AlphaNumeric,
            )],
        ),
        (
            "Ⅻ",
            vec![AnalysisToken::new("xii", 0, 3, TokenType::AlphaNumeric)],
        ),
        (
            "ＡＢＣ",
            vec![AnalysisToken::new("abc", 0, 9, TokenType::AlphaNumeric)],
        ),
        // Symbols the tokenizer drops before NFKC could turn them into words.
        ("① ㎏ ½", vec![]),
        // Composed and decomposed é are one term.
        (
            "e\u{301}",
            vec![AnalysisToken::new("é", 0, 3, TokenType::AlphaNumeric)],
        ),
        (
            "é",
            vec![AnalysisToken::new("é", 0, 2, TokenType::AlphaNumeric)],
        ),
    ];
    for (input, want) in tests {
        assert_eq!(analyze(input), want, "{input:?}");
    }
}

/// FNV-1a 64 of Dgraph's term analysis in Go (bleve v2.5.7's unicode tokenizer, lowercase filter and NFKC over
/// golang.org/x/text v0.41.0), by a Go program, per plane: for every scalar value `c` of the plane and each context
/// `x` of none and `a`, the input `x c` gives `c` as four little-endian bytes, `x`'s byte (0 for none), and for
/// each token its term's length as four little-endian bytes, the term's bytes, its start and end as four
/// little-endian bytes each and its type as one byte, then a 0xff; beside the hash, the number of tokens.
///
/// The cases above take a few compatibility characters; this takes every rune that segments as a token, so every
/// decomposition and composition the normalization's tables hold for one rune, or for `a` and a rune, is compared
/// with Go's. The contexts put nothing after the rune, where bleve's lowercasing differs from the port's (ledger
/// 16, 30).
const GO_PLANE_HASHES: [(u32, usize, u64); 5] = [
    (0, 141_536, 0xcb84_6458_2521_fc0e),
    (1, 73_430, 0x5754_37a5_9d19_321f),
    (2, 172_308, 0xceed_2add_6cb2_d70b),
    (3, 65_536, 0x6e9d_8090_9a6e_5b25),
    (14, 65_536, 0xd404_c723_40e9_9a51),
];

struct Fnv(u64);

impl Fnv {
    fn byte(&mut self, b: u8) {
        self.0 = (self.0 ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3);
    }

    fn u32(&mut self, x: u32) {
        x.to_le_bytes().into_iter().for_each(|b| self.byte(b));
    }
}

#[test]
fn test_unicode_normalization_every_scalar() {
    for (plane, want_tokens, want) in GO_PLANE_HASHES {
        let mut h = Fnv(0xcbf2_9ce4_8422_2325);
        let mut tokens = 0;
        for c in (plane << 16..(plane + 1) << 16).filter_map(char::from_u32) {
            for ctx in ["", "a"] {
                h.u32(u32::from(c));
                h.byte(ctx.bytes().next().unwrap_or(0));
                for t in analyze(&format!("{ctx}{c}")) {
                    tokens += 1;
                    h.u32(t.term().len() as u32);
                    t.term().bytes().for_each(|b| h.byte(b));
                    h.u32(t.start() as u32);
                    h.u32(t.end() as u32);
                    h.byte(t.typ() as u8);
                }
                h.byte(0xff);
            }
        }
        assert_eq!((tokens, h.0), (want_tokens, want), "plane {plane}");
    }
}
