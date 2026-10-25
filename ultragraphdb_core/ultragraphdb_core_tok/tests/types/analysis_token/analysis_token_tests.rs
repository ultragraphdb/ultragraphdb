/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_tok::{AnalysisToken, SegmentType, TokenType};

#[test]
fn test_analysis_token_fields() {
    let t = AnalysisToken::new("katz", 3, 9, TokenType::Ideographic);
    assert_eq!(
        (t.term(), t.start(), t.end(), t.typ()),
        ("katz", 3, 9, TokenType::Ideographic)
    );
    let t = AnalysisToken::of(String::from("é"));
    assert_eq!(
        (t.term(), t.start(), t.end(), t.typ()),
        ("é", 0, 0, TokenType::AlphaNumeric)
    );
}

/// Two tokens are equal when every field is: term, both offsets and the type.
#[test]
fn test_analysis_token_equality() {
    let t = AnalysisToken::new("a", 1, 2, TokenType::Single);
    assert_eq!(t, AnalysisToken::new("a", 1, 2, TokenType::Single));
    for other in [
        AnalysisToken::new("b", 1, 2, TokenType::Single),
        AnalysisToken::new("a", 0, 2, TokenType::Single),
        AnalysisToken::new("a", 1, 3, TokenType::Single),
        AnalysisToken::new("a", 1, 2, TokenType::Double),
    ] {
        assert_ne!(t, other, "{other:?}");
    }
}

/// bleve's `TokenType` and segment's word types are Go `iota` constants; the golden streams carry them as numbers.
#[test]
fn test_type_numbers() {
    let token_types = [
        TokenType::AlphaNumeric,
        TokenType::Ideographic,
        TokenType::Numeric,
        TokenType::DateTime,
        TokenType::Shingle,
        TokenType::Single,
        TokenType::Double,
        TokenType::Boolean,
        TokenType::Ip,
    ];
    let numbers: Vec<u8> = token_types.iter().map(|t| *t as u8).collect();
    assert_eq!(numbers, (0..=8).collect::<Vec<u8>>());
    let segment_types = [
        SegmentType::None,
        SegmentType::Number,
        SegmentType::Letter,
        SegmentType::Kana,
        SegmentType::Ideo,
    ];
    let numbers: Vec<u8> = segment_types.iter().map(|t| *t as u8).collect();
    assert_eq!(numbers, (0..=4).collect::<Vec<u8>>());
}
