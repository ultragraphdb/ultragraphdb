/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, tok/stopwords_test.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_core_tok::{AnalysisToken, filter_stopwords, stop_words};

fn stream(terms: &[&str]) -> Vec<AnalysisToken> {
    terms.iter().map(|t| AnalysisToken::of(*t)).collect()
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
    // bleve v2.5.7's stop_<lang> token maps, as a Go program over its registry reads them: the size and the first
    // and last word in byte order.
    let langs: [(&str, usize, &str, &str); 28] = [
        ("ar", 119, "أ", "يكون"),
        ("bg", 190, "а", "я"),
        ("ca", 218, "a", "és"),
        ("ckb", 62, "ئێمە", "ی"),
        ("cs", 171, "a", "články"),
        ("da", 94, "ad", "været"),
        ("de", 231, "aber", "über"),
        ("el", 75, "αλλα", "ωσ"),
        ("en", 174, "a", "yourselves"),
        ("es", 308, "a", "éramos"),
        ("eu", 98, "al", "zuten"),
        ("fa", 308, "اباد", "گيري"),
        ("fi", 229, "ei", "yli"),
        ("fr", 164, "ai", "êtes"),
        ("ga", 109, "a", "ónár"),
        ("gl", 160, "a", "ós"),
        ("hi", 225, "अंदर", "होने"),
        ("hu", 198, "a", "őket"),
        ("hy", 45, "այդ", "և"),
        ("id", 355, "ada", "yang"),
        ("it", 279, "a", "è"),
        ("nl", 101, "aan", "zou"),
        ("no", 172, "alle", "å"),
        ("pt", 203, "a", "éramos"),
        ("ro", 230, "acea", "ţie"),
        ("ru", 159, "а", "я"),
        ("sv", 114, "alla", "över"),
        ("tr", 209, "acaba", "şöyle"),
    ];
    for (lang, len, first, last) in langs {
        let words = stop_words(lang).unwrap_or_else(|| panic!("no stop words for {lang}"));
        assert!(words.windows(2).all(|w| w[0] < w[1]), "{lang} not sorted");
        assert_eq!(
            (words.len(), words.first(), words.last()),
            (len, Some(&first), Some(&last)),
            "{lang}"
        );
    }
    for lang in ["zh", "ja", "ko", "", "EN", "en-US"] {
        assert!(stop_words(lang).is_none(), "{lang:?}");
    }
}
