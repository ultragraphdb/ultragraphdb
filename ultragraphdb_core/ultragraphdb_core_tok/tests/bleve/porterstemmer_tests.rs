/*
 * Copyright (c) 2013 Charles Iliya Krempeaux <charles@reptile.ca> :: http://changelog.ca/
 * Copyright (c) 2014 Couchbase, Inc.
 * SPDX-License-Identifier: MIT AND Apache-2.0
 *
 * Ported from github.com/blevesearch/go-porterstemmer v1.0.3, porterstemmer_stem_without_lower_casing_test.go,
 * porterstemmer_fixes_test.go and porterstemmer_fuzz_test.go, and from bleve v2.5.7,
 * analysis/token/porter/porter_test.go. Changed for ultragraphdb: translated to Rust; Porter's published examples
 * and the fuzz range checked against Go's output. The MIT notice is in LICENSE-PORTERSTEMMER.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_core_tok::{AnalysisToken, Stemmer, TokenType};

use crate::bleve::check_stems;

#[test]
fn test_porter_stemmer() {
    check_stems(
        "stemmer_porter",
        &[
            // TestStemWithoutLowerCasing, its empty case included.
            ("controll", "control"),
            ("roll", "roll"),
            ("", ""),
            // TestStemStringIon: "ion" once panicked.
            ("ion", "ion"),
            // bleve's TestPorterStemmer, but its KeyWord token, which Dgraph never marks.
            ("walking", "walk"),
            ("talked", "talk"),
            ("business", "busi"),
            ("cat", "cat"),
            ("done", "done"),
            ("marty", "marti"),
            // Porter's examples for each step, as Go stems them.
            ("caresses", "caress"),
            ("ponies", "poni"),
            ("ties", "ti"),
            ("caress", "caress"),
            ("cats", "cat"),
            ("feed", "feed"),
            ("agreed", "agre"),
            ("plastered", "plaster"),
            ("bled", "bled"),
            ("motoring", "motor"),
            ("sing", "sing"),
            ("conflated", "conflat"),
            ("troubled", "troubl"),
            ("sized", "size"),
            ("hopping", "hop"),
            ("tanned", "tan"),
            ("falling", "fall"),
            ("hissing", "hiss"),
            ("fizzed", "fizz"),
            ("failing", "fail"),
            ("filing", "file"),
            ("happy", "happi"),
            ("sky", "sky"),
            ("relational", "relat"),
            ("conditional", "condit"),
            ("rational", "ration"),
            ("valenci", "valenc"),
            ("hesitanci", "hesit"),
            ("digitizer", "digit"),
            ("conformabli", "conform"),
            ("radicalli", "radic"),
            ("differentli", "differ"),
            ("vileli", "vile"),
            ("analogousli", "analog"),
            ("vietnamization", "vietnam"),
            ("predication", "predic"),
            ("operator", "oper"),
            ("feudalism", "feudal"),
            ("decisiveness", "decis"),
            ("hopefulness", "hope"),
            ("callousness", "callous"),
            ("formaliti", "formal"),
            ("sensitiviti", "sensit"),
            ("sensibiliti", "sensibl"),
            ("triplicate", "triplic"),
            ("formative", "form"),
            ("formalize", "formal"),
            ("electriciti", "electr"),
            ("electrical", "electr"),
            ("hopeful", "hope"),
            ("goodness", "good"),
            ("revival", "reviv"),
            ("allowance", "allow"),
            ("inference", "infer"),
            ("airliner", "airlin"),
            ("gyroscopic", "gyroscop"),
            ("adjustable", "adjust"),
            ("defensible", "defens"),
            ("irritant", "irrit"),
            ("replacement", "replac"),
            ("adjustment", "adjust"),
            ("dependent", "depend"),
            ("adoption", "adopt"),
            ("homologou", "homolog"),
            ("communism", "commun"),
            ("activate", "activ"),
            ("angulariti", "angular"),
            ("homologous", "homolog"),
            ("effective", "effect"),
            ("bowdlerize", "bowdler"),
            ("probate", "probat"),
            ("rate", "rate"),
            ("cease", "ceas"),
            // No lowercasing: an upper-case suffix is not one, an upper-case stem keeps its case.
            ("Running", "Run"),
            ("CATS", "CATS"),
            // A `y` after a vowel or a consonant `y` is a consonant, one after a vowel `y` a vowel.
            ("syzygy", "syzygi"),
        ],
        &[
            ("yyyy", "yyyi"),
            ("ayyy", "ayyi"),
            ("ies", "ie"),
            ("eed", "e"),
        ],
    );
}

/// go-porterstemmer's `TestStemFuzz`, which asks only that no term of one to five letters panics, here over every
/// term of one to four letters `a` to `z` with the stems compared to Go's: FNV-1a 64 over each stem's bytes and a
/// 0xff, in the fuzz order (by length, then alphabetically), as bleve's `stemmer_porter` gives them.
#[test]
fn test_porter_stem_fuzz() {
    const GO_TERMS: usize = 475_254;
    const GO_CHANGED: usize = 30_960;
    const GO_HASH: u64 = 0x11e7_6279_923d_7d27;
    let s = Stemmer::named("stemmer_porter").unwrap();
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let (mut terms, mut changed) = (0, 0);
    let mut term = Vec::new();
    for len in 1..=4 {
        term.clear();
        term.resize(len, b'a');
        loop {
            let input = std::str::from_utf8(&term).unwrap();
            let stem = s.stem(input);
            terms += 1;
            changed += usize::from(stem != input);
            for b in stem.bytes().chain([0xff]) {
                hash = (hash ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3);
            }
            // The next term of this length, as incrementBytes counts.
            match term.iter().rposition(|&b| b != b'z') {
                None => break,
                Some(i) => {
                    term[i] += 1;
                    term[i + 1..].fill(b'a');
                }
            }
        }
    }
    assert_eq!((terms, changed), (GO_TERMS, GO_CHANGED));
    assert_eq!(hash, GO_HASH);
}

#[test]
fn test_porter_filter_keeps_offsets() {
    let s = Stemmer::named("stemmer_porter").unwrap();
    let out = s.filter(vec![
        AnalysisToken::new("walking", 0, 7, TokenType::AlphaNumeric),
        AnalysisToken::new("42", 8, 10, TokenType::Numeric),
        AnalysisToken::new("dogs", 11, 15, TokenType::AlphaNumeric),
    ]);
    assert_eq!(
        out,
        vec![
            AnalysisToken::new("walk", 0, 7, TokenType::AlphaNumeric),
            AnalysisToken::new("42", 8, 10, TokenType::Numeric),
            AnalysisToken::new("dog", 11, 15, TokenType::AlphaNumeric),
        ]
    );
    assert_eq!(s.filter(Vec::new()), Vec::new());
}
