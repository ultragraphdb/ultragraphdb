/*
 * Copyright (c) 2014 Couchbase, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from bleve v2.5.7, analysis/lang/cjk/cjk_bigram_test.go (TestCJKBigramFilter). Changed for ultragraphdb:
 * translated to Rust; the cases with `outputUnigram` off, as Dgraph's `cjk_bigram` has it, without the token
 * positions the port does not keep, and further streams at the edges with the output bleve's filter gives them.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_core_tok::{AnalysisToken, Stemmer, TokenType};

fn tok(term: &str, start: usize, end: usize, typ: TokenType) -> AnalysisToken {
    AnalysisToken::new(term, start, end, typ)
}

fn ideo(term: &str, start: usize, end: usize) -> AnalysisToken {
    tok(term, start, end, TokenType::Ideographic)
}

fn single(term: &str, start: usize, end: usize) -> AnalysisToken {
    tok(term, start, end, TokenType::Single)
}

fn double(term: &str, start: usize, end: usize) -> AnalysisToken {
    tok(term, start, end, TokenType::Double)
}

#[test]
fn test_cjk_bigram_filter() {
    let tests: Vec<(&str, Vec<AnalysisToken>, Vec<AnalysisToken>)> = vec![
        (
            "non-adjacent terms are not combined",
            vec![ideo("こ", 0, 3), ideo("ん", 5, 8)],
            vec![single("こ", 0, 3), single("ん", 5, 8)],
        ),
        (
            "adjacent runes overlap in bigrams",
            vec![
                ideo("こ", 0, 3),
                ideo("ん", 3, 6),
                ideo("に", 6, 9),
                ideo("ち", 9, 12),
                ideo("は", 12, 15),
                ideo("世", 15, 18),
                ideo("界", 18, 21),
            ],
            vec![
                double("こん", 0, 6),
                double("んに", 3, 9),
                double("にち", 6, 12),
                double("ちは", 9, 15),
                double("は世", 12, 18),
                double("世界", 15, 21),
            ],
        ),
        (
            "a non-ideographic token ends a run and passes through",
            vec![
                ideo("こ", 0, 3),
                ideo("ん", 3, 6),
                ideo("に", 6, 9),
                ideo("ち", 9, 12),
                ideo("は", 12, 15),
                tok("cat", 12, 15, TokenType::AlphaNumeric),
                ideo("世", 18, 21),
                ideo("界", 21, 24),
            ],
            vec![
                double("こん", 0, 6),
                double("んに", 3, 9),
                double("にち", 6, 12),
                double("ちは", 9, 15),
                tok("cat", 12, 15, TokenType::AlphaNumeric),
                double("世界", 18, 24),
            ],
        ),
        (
            "the runes of one token are bigrammed",
            vec![ideo("パイプライン", 0, 18)],
            vec![
                double("パイ", 0, 6),
                double("イプ", 3, 9),
                double("プラ", 6, 12),
                double("ライ", 9, 15),
                double("イン", 12, 18),
            ],
        ),
        // The edges, beyond bleve's table.
        ("an empty stream", vec![], vec![]),
        (
            "a lone rune is a unigram",
            vec![ideo("人", 0, 3)],
            vec![single("人", 0, 3)],
        ),
        (
            "a lone rune after a run is a unigram, the run's last rune is not",
            vec![ideo("世", 0, 3), ideo("界", 3, 6), ideo("人", 7, 10)],
            vec![double("世界", 0, 6), single("人", 7, 10)],
        ),
        (
            "a lone rune before a non-ideographic token is flushed before it",
            vec![ideo("人", 0, 3), tok("42", 3, 5, TokenType::Numeric)],
            vec![single("人", 0, 3), tok("42", 3, 5, TokenType::Numeric)],
        ),
        (
            "non-ideographic tokens alone pass through",
            vec![
                tok("abc", 0, 3, TokenType::AlphaNumeric),
                tok("한국", 4, 10, TokenType::AlphaNumeric),
            ],
            vec![
                tok("abc", 0, 3, TokenType::AlphaNumeric),
                tok("한국", 4, 10, TokenType::AlphaNumeric),
            ],
        ),
        (
            "a supplementary-plane rune is one rune",
            vec![ideo("𠀀", 0, 4), ideo("字", 4, 7)],
            vec![double("𠀀字", 0, 7)],
        ),
    ];
    let cjk = Stemmer::named("cjk_bigram").unwrap();
    for (name, input, output) in tests {
        assert_eq!(cjk.filter(input), output, "{name}");
    }
}

#[test]
fn test_cjk_bigram_stem_keeps_the_term() {
    let cjk = Stemmer::named("cjk_bigram").unwrap();
    assert_eq!(cjk.stem("世界"), "世界");
    assert_eq!(cjk.stem(""), "");
}
