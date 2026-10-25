/*
 * Copyright (c) 2014 Couchbase, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from bleve v2.5.7, analysis/tokenizer/unicode/unicode_test.go (TestUnicode, TestConvertType). Changed for
 * ultragraphdb: translated to Rust; the tokenizer is reached through `analyze`, so the terms are lowercased, and
 * further inputs carry the offsets and types bleve's analyzer gives them.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_core_tok::{AnalysisToken, TokenType, analyze};

fn alnum(term: &str, start: usize, end: usize) -> AnalysisToken {
    AnalysisToken::new(term, start, end, TokenType::AlphaNumeric)
}

fn ideo(term: &str, start: usize, end: usize) -> AnalysisToken {
    AnalysisToken::new(term, start, end, TokenType::Ideographic)
}

fn num(term: &str, start: usize, end: usize) -> AnalysisToken {
    AnalysisToken::new(term, start, end, TokenType::Numeric)
}

#[test]
fn test_unicode() {
    let tests: Vec<(&str, Vec<AnalysisToken>)> = vec![
        // bleve's table.
        (
            "Hello World",
            vec![alnum("hello", 0, 5), alnum("world", 6, 11)],
        ),
        ("steven's", vec![alnum("steven's", 0, 8)]),
        (
            "こんにちは世界",
            vec![
                ideo("こ", 0, 3),
                ideo("ん", 3, 6),
                ideo("に", 6, 9),
                ideo("ち", 9, 12),
                ideo("は", 12, 15),
                ideo("世", 15, 18),
                ideo("界", 18, 21),
            ],
        ),
        ("age 25", vec![alnum("age", 0, 3), num("25", 4, 6)]),
        ("カ", vec![ideo("カ", 0, 3)]),
        // Offsets count the dropped segments.
        (
            "Now  is the.\n End.",
            vec![
                alnum("now", 0, 3),
                alnum("is", 5, 7),
                alnum("the", 8, 11),
                alnum("end", 14, 17),
            ],
        ),
        ("3.5 cats", vec![num("3.5", 0, 3), alnum("cats", 4, 8)]),
        ("cat3.5", vec![alnum("cat3.5", 0, 6)]),
        ("サッカ", vec![ideo("サッカ", 0, 9)]),
        ("한국어", vec![alnum("한국어", 0, 9)]),
        ("e-mail", vec![alnum("e", 0, 1), alnum("mail", 2, 6)]),
        ("", vec![]),
        ("  ", vec![]),
    ];
    for (input, want) in tests {
        assert_eq!(analyze(input), want, "{input:?}");
    }
}

#[test]
fn test_convert_type() {
    // TestConvertType through the segments that carry each type: Ideo (Han, Hiragana and Katakana) is Ideographic,
    // Number is Numeric, Letter (Hangul too) is AlphaNumeric. segment v0.9.1 never types a segment Kana.
    let types: Vec<TokenType> = analyze("世 か カ 7 x 한").iter().map(|t| t.typ()).collect();
    assert_eq!(
        types,
        vec![
            TokenType::Ideographic,
            TokenType::Ideographic,
            TokenType::Ideographic,
            TokenType::Numeric,
            TokenType::AlphaNumeric,
            TokenType::AlphaNumeric,
        ]
    );
}
