/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, tok/stemmers_test.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_core_tok::{AnalysisToken, filter_stemmers};

fn stream(terms: &[&str]) -> Vec<AnalysisToken> {
    terms
        .iter()
        .map(|t| AnalysisToken::of(t.as_bytes()))
        .collect()
}

#[test]
fn test_filter_stemmers() {
    let tests: [(&str, Vec<AnalysisToken>, Vec<AnalysisToken>); 5] = [
        (
            "en",
            stream(&[
                "the", "quick", "brown", "foxes", "jump", "over", "the", "big", "dogs",
            ]),
            stream(&[
                "the", "quick", "brown", "fox", "jump", "over", "the", "big", "dog",
            ]),
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
                "deseándol",
                "much",
                "alegrí",
                "a",
                "las",
                "señorit",
                "y",
                "los",
                "señor",
                "programador",
                "de",
                "Dgraph",
            ]),
        ),
        (
            "x-klingon",
            stream(&["tlhIngan", "maH!"]),
            stream(&["tlhIngan", "maH!"]),
        ),
        ("en", stream(&[]), stream(&[])),
        ("", stream(&[""]), stream(&[""])),
    ];
    for (lang, input, out) in tests {
        assert_eq!(filter_stemmers(lang, input), out, "{lang}");
    }
}
