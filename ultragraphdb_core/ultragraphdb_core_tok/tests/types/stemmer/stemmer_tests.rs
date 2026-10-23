/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use serde::Deserialize;
use ultragraphdb_core_tok::{AnalysisToken, Stemmer, analyze};

use crate::golden::cases;
use crate::utils::bleve_tests::check_streams;

#[derive(Debug, Clone, Deserialize)]
struct StemmerCase {
    filter: String,
    words: Vec<(String, String)>,
}

#[test]
fn test_stemmers_golden() {
    let cases: Vec<StemmerCase> = cases("tok_stemmers");
    assert_eq!(cases.len(), 18);
    let mut failures = Vec::new();
    for c in &cases {
        let s = Stemmer::named(&c.filter).unwrap_or_else(|| panic!("no stemmer {}", c.filter));
        assert_eq!(s.name(), c.filter);
        let bad: Vec<String> = c
            .words
            .iter()
            .filter(|(i, o)| s.stem(i) != *o)
            .map(|(i, o)| format!("{i} -> {o}, got {}", s.stem(i)))
            .collect();
        if !bad.is_empty() {
            failures.push(format!(
                "{}: {} of {} differ, e.g. {:?}",
                c.filter,
                bad.len(),
                c.words.len(),
                &bad[..bad.len().min(5)]
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn test_cjk_bigram_golden() {
    let cjk = Stemmer::named("cjk_bigram").unwrap();
    check_streams("tok_cjk_bigram", |input| cjk.filter(analyze(input)));
}

#[test]
fn test_stemmer_for_lang() {
    assert_eq!(Stemmer::for_lang("en").unwrap().name(), "stemmer_porter");
    assert_eq!(Stemmer::for_lang("zh").unwrap().name(), "cjk_bigram");
    assert!(Stemmer::for_lang("bg").is_none());
    assert!(Stemmer::named("stemmer_en_snowball").is_none());
    let out = Stemmer::for_lang("de")
        .unwrap()
        .filter(vec![AnalysisToken::of("katzen")]);
    assert_eq!(out[0].term(), "katz");
}
