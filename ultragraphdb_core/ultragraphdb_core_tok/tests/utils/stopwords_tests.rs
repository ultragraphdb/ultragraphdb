/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, tok/stopwords_test.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_core_tok::{AnalysisToken, filter_stopwords, stop_words};

fn stream(terms: &[&str]) -> Vec<AnalysisToken> {
    terms
        .iter()
        .map(|t| AnalysisToken::of(t.as_bytes()))
        .collect()
}

#[test]
fn test_filter_stopwords() {
    let tests: [(&str, Vec<AnalysisToken>, Vec<AnalysisToken>); 6] = [
        (
            "en",
            stream(&[
                "the", "quick", "brown", "foxes", "jump", "over", "the", "big", "dogs",
            ]),
            stream(&["quick", "brown", "foxes", "jump", "big", "dogs"]),
        ),
        (
            "es",
            stream(&[
                "deseándoles",
                "muchas",
                "alegrías",
                "a",
                "las",
                "señoritas",
                "y",
                "los",
                "señores",
                "programadores",
                "de",
                "Dgraph",
            ]),
            stream(&[
                "deseándoles",
                "muchas",
                "alegrías",
                "señoritas",
                "señores",
                "programadores",
                "Dgraph",
            ]),
        ),
        (
            "x-klingon",
            stream(&["tlhIngan", "maH!"]),
            stream(&["tlhIngan", "maH!"]),
        ),
        ("en", stream(&["same"]), stream(&[])),
        ("en", stream(&[]), stream(&[])),
        ("", stream(&[""]), stream(&[""])),
    ];
    for (lang, input, out) in tests {
        assert_eq!(filter_stopwords(lang, input), out, "{lang}");
    }
}

#[test]
fn test_stop_word_languages() {
    let langs = [
        "ar", "bg", "ca", "ckb", "cs", "da", "de", "el", "en", "es", "eu", "fa", "fi", "fr", "ga",
        "gl", "hi", "hu", "hy", "id", "it", "nl", "no", "pt", "ro", "ru", "sv", "tr",
    ];
    for lang in langs {
        let words = stop_words(lang).unwrap_or_else(|| panic!("no stop words for {lang}"));
        assert!(words.windows(2).all(|w| w[0] < w[1]), "{lang} not sorted");
    }
    assert!(stop_words("zh").is_none());
    assert_eq!(stop_words("en").unwrap().len(), 174);
}
