/*
 * Copyright (c) 2014 Couchbase, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from bleve v2.5.7, analysis/token/stop/stop_test.go (TestStopWordsFilter). Changed for ultragraphdb:
 * translated to Rust; bleve's stop list `a in the` is the English one's, and the filter is reached through
 * `filter_stopwords`.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_core_tok::{AnalysisToken, TokenType, filter_stopwords};

use crate::tables::stop_word_tables_tests::GO_STOP_WORDS;

fn alnum(term: &str, start: usize, end: usize) -> AnalysisToken {
    AnalysisToken::new(term, start, end, TokenType::AlphaNumeric)
}

#[test]
fn test_stop_words_filter() {
    let input = vec![
        alnum("a", 0, 1),
        alnum("walk", 2, 6),
        alnum("in", 7, 9),
        alnum("the", 10, 13),
        alnum("park", 14, 18),
    ];
    assert_eq!(
        filter_stopwords("en", input),
        vec![alnum("walk", 2, 6), alnum("park", 14, 18)]
    );
}

#[test]
fn test_stop_words_filter_matches_bytes() {
    // The map is keyed by the exact term: an upper-case or a stemmed form is no stop word, every repeat of one is.
    let input = vec![
        alnum("The", 0, 3),
        alnum("the", 4, 7),
        alnum("thes", 8, 12),
        alnum("the", 13, 16),
        alnum("", 17, 17),
    ];
    assert_eq!(
        filter_stopwords("en", input),
        vec![alnum("The", 0, 3), alnum("thes", 8, 12), alnum("", 17, 17)]
    );
}

#[test]
fn test_stop_words_filter_every_language() {
    // Each language's first and last stop word, as bleve's map has them, go; a word of none of them stays.
    for (lang, _, _, first, last) in GO_STOP_WORDS {
        let input = vec![
            alnum(first, 0, 1),
            alnum("dgraph", 2, 8),
            alnum(last, 9, 10),
        ];
        assert_eq!(
            filter_stopwords(lang, input),
            vec![alnum("dgraph", 2, 8)],
            "{lang}"
        );
    }
    // A language without stop words keeps every token, as does an empty stream.
    let input = vec![alnum("the", 0, 3), alnum("的", 4, 7)];
    assert_eq!(filter_stopwords("zh", input.clone()), input);
    assert_eq!(filter_stopwords("en", Vec::new()), Vec::new());
}
