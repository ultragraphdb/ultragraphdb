/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use serde::Deserialize;
use ultragraphdb_core_tok::{AnalysisToken, Stemmer, TokenType, analyze};

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
    // Dgraph's langStemmers (tok/stemmers.go), whole.
    let lang_stemmers = [
        ("ar", "stemmer_ar"),
        ("ckb", "stemmer_ckb"),
        ("da", "stemmer_da_snowball"),
        ("de", "stemmer_de_light"),
        ("en", "stemmer_porter"),
        ("es", "stemmer_es_light"),
        ("fi", "stemmer_fi_snowball"),
        ("fr", "stemmer_fr_light"),
        ("hi", "stemmer_hi"),
        ("hu", "stemmer_hu_snowball"),
        ("it", "stemmer_it_light"),
        ("ja", "cjk_bigram"),
        ("ko", "cjk_bigram"),
        ("nl", "stemmer_nl_snowball"),
        ("no", "stemmer_no_snowball"),
        ("pt", "stemmer_pt_light"),
        ("ro", "stemmer_ro_snowball"),
        ("ru", "stemmer_ru_snowball"),
        ("sv", "stemmer_sv_snowball"),
        ("tr", "stemmer_tr_snowball"),
        ("zh", "cjk_bigram"),
    ];
    for (lang, name) in lang_stemmers {
        let s = Stemmer::for_lang(lang).unwrap_or_else(|| panic!("no stemmer for {lang}"));
        assert_eq!(s.name(), name, "{lang}");
        assert_eq!(Stemmer::named(name), Some(s), "{name}");
    }
    // A language base Dgraph has no stemmer for, and what is not a base: the lookup is exact.
    for lang in ["bg", "el", "", "EN", "en-US", "zz", "zh-Hant"] {
        assert_eq!(Stemmer::for_lang(lang), None, "{lang:?}");
    }
    for name in ["stemmer_en_snowball", "porter", "", "STEMMER_PORTER"] {
        assert_eq!(Stemmer::named(name), None, "{name:?}");
    }
    let out = Stemmer::for_lang("de")
        .unwrap()
        .filter(vec![AnalysisToken::of("katzen")]);
    assert_eq!(out, vec![AnalysisToken::of("katz")]);
    // A stemmer changes the term and keeps the token's offsets and type.
    let out = Stemmer::for_lang("en")
        .unwrap()
        .filter(vec![AnalysisToken::new(
            "foxes",
            4,
            9,
            TokenType::AlphaNumeric,
        )]);
    assert_eq!(
        out,
        vec![AnalysisToken::new("fox", 4, 9, TokenType::AlphaNumeric)]
    );
    // cjk_bigram works on the stream; one term it keeps.
    assert_eq!(Stemmer::named("cjk_bigram").unwrap().stem("他是"), "他是");
    assert!(
        Stemmer::named("cjk_bigram")
            .unwrap()
            .filter(Vec::new())
            .is_empty()
    );
}
