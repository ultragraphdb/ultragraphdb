/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, tok/tok_test.go. Changed for ultragraphdb: translated to Rust; the golden vectors of
 * tools/golden/tok.go added.
 * Copyright (c) 2026. Marvin Hansen.
 */

use blake2::{Blake2b256, Digest};
use serde::Deserialize;
use ultragraphdb_core_tok::{
    IDENT_FULL_TEXT, IDENT_NGRAM, IDENT_TERM, TOKENIZER_NAMES, TokErrorEnum, Tokenizer,
    build_ngram_query_tokens, build_tokens, encode_geo_tokens, encode_regex_tokens,
    get_full_text_tokens, get_ngram_query_tokens, get_ngram_tokens, get_tokenizer,
    get_tokenizer_by_id, get_tokenizer_for_lang, get_tokenizers,
};
use ultragraphdb_core_types::{BigFloat, DateTime, Geometry, Layout, TypeId, Val, Value, convert};

use crate::golden::{LOWERCASE_DEVIATIONS, cases, unhex, untext};

fn enc(id: u8, s: &str) -> Vec<u8> {
    let mut v = vec![id];
    v.extend_from_slice(s.as_bytes());
    v
}

/// The ngram tokens of `shingles`, in the order given.
fn ngrams(shingles: &[&str]) -> Vec<Vec<u8>> {
    shingles.iter().map(|s| enc(IDENT_NGRAM, s)).collect()
}

fn tokenizer(name: &str) -> Tokenizer {
    get_tokenizer(name).unwrap_or_else(|| panic!("no tokenizer {name}"))
}

fn tokens_for_lang(name: &str, lang: &str, s: &str) -> Vec<Vec<u8>> {
    build_tokens(
        &Value::str(s),
        &get_tokenizer_for_lang(&tokenizer(name), lang),
    )
    .unwrap()
}

fn assert_unique(tokens: &[Vec<u8>]) {
    let mut sorted = tokens.to_vec();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted.len(), tokens.len(), "tokens are not unique");
}

fn check_sorted_and_unique(tokens: &[Vec<u8>]) {
    assert!(
        tokens.windows(2).all(|w| w[0] < w[1]),
        "tokens are not sorted and unique"
    );
}

fn time(s: &str) -> DateTime {
    match convert(
        &Val::new(TypeId::STRING, Value::Bytes(s.as_bytes().to_vec())),
        TypeId::DATETIME,
    )
    .unwrap()
    .into_value()
    {
        Some(Value::DateTime(t)) => t,
        other => panic!("{other:?}"),
    }
}

#[test]
fn test_int_encoding() {
    let mut ints: Vec<i64> = vec![
        (1 << 24) + 10,
        -(1 << 24) - 1,
        i64::MAX,
        i64::MIN,
        1,
        2,
        3,
        4,
        -1,
        -2,
        -3,
        0,
        234,
        10000,
        123,
        -1543,
    ];
    ints.sort();
    let int = tokenizer("int");
    let tokens: Vec<Vec<u8>> = ints
        .iter()
        .map(|i| int.tokens(&Value::Int(*i)).unwrap().remove(0))
        .collect();
    for i in 1..tokens.len() {
        assert!(
            tokens[i - 1] < tokens[i],
            "{} {:?} vs {} {:?}",
            ints[i - 1],
            tokens[i - 1],
            ints[i],
            tokens[i]
        );
    }
}

#[test]
fn test_full_text_tokenizer() {
    let tokens = tokens_for_lang("fulltext", "en", "Stemming works!");
    assert_eq!(
        tokens,
        vec![enc(IDENT_FULL_TEXT, "stem"), enc(IDENT_FULL_TEXT, "work")]
    );
}

#[test]
fn test_date_time_tokenizers() {
    let dt = time("2017-01-01T12:12:12Z");
    // Go checks only the lengths, 1 + 2 bytes per field; the fields are big-endian u16s: 2017 is 0x07E1.
    for (name, token) in [
        ("hour", &b"\x43\x07\xe1\x00\x01\x00\x01\x00\x0c"[..]),
        ("day", &b"\x42\x07\xe1\x00\x01\x00\x01"[..]),
        ("month", &b"\x41\x07\xe1\x00\x01"[..]),
        ("year", &b"\x04\x07\xe1"[..]),
    ] {
        let tokens = build_tokens(&Value::DateTime(dt), &tokenizer(name)).unwrap();
        assert_eq!(tokens, vec![token.to_vec()], "{name}");
    }
}

#[test]
fn test_full_text_tokenizer_lang() {
    let tokens = tokens_for_lang("fulltext", "de", "Katzen und Auffassung und Auffassung");
    // tokens should be sorted and unique
    assert_eq!(
        tokens,
        vec![
            enc(IDENT_FULL_TEXT, "auffassung"),
            enc(IDENT_FULL_TEXT, "katz")
        ]
    );
}

#[test]
fn test_term_tokenizer() {
    let tokens = build_tokens(&Value::str("Tokenizer works works!"), &tokenizer("term")).unwrap();
    assert_eq!(
        tokens,
        vec![enc(IDENT_TERM, "tokenizer"), enc(IDENT_TERM, "works")]
    );
}

#[test]
fn test_trigram_tokenizer() {
    let t = tokenizer("trigram");
    let tokens = build_tokens(&Value::str("Dgraph rocks!"), &t).unwrap();
    assert_eq!(tokens.len(), 11);
    let mut expected: Vec<Vec<u8>> = [
        "Dgr", "gra", "rap", "aph", "ph ", "h r", " ro", "roc", "ock", "cks", "ks!",
    ]
    .iter()
    .map(|s| enc(t.identifier(), s))
    .collect();
    expected.sort();
    assert_eq!(tokens, expected);
}

#[test]
fn test_get_full_text_tokens() {
    let val = "Our chief weapon is surprise...surprise and fear...fear and surprise....\
               Our two weapons are fear and surprise...and ruthless efficiency.... \
               Our three weapons are fear, surprise, and ruthless efficiency...";
    let t = get_tokenizer_for_lang(&tokenizer("fulltext"), "en");
    let tokens = t.tokens(&Value::str(val)).unwrap();
    let mut expected: Vec<Vec<u8>> = [
        "chief", "weapon", "surpris", "fear", "ruthless", "effici", "two", "three",
    ]
    .iter()
    .map(|s| s.as_bytes().to_vec())
    .collect();
    expected.sort();
    // ensure that tokens are sorted and unique
    assert_eq!(tokens, expected);
}

#[test]
fn test_get_full_text_tokens1() {
    // Go checks the count alone; the tokens are Dgraph's.
    let want = vec![
        enc(IDENT_FULL_TEXT, "brown"),
        enc(IDENT_FULL_TEXT, "fox"),
        enc(IDENT_FULL_TEXT, "quick"),
    ];
    assert_eq!(
        get_full_text_tokens(&["Quick brown fox"], "en").unwrap(),
        want
    );
}

#[test]
fn test_get_full_text_tokens_invalid_lang() {
    // An unknown language reverts to English.
    let want = vec![
        enc(IDENT_FULL_TEXT, "brown"),
        enc(IDENT_FULL_TEXT, "fox"),
        enc(IDENT_FULL_TEXT, "quick"),
    ];
    assert_eq!(
        get_full_text_tokens(&["Quick brown fox"], "xxx_such_language").unwrap(),
        want
    );
}

#[test]
fn test_get_full_text_tokens_argument_count() {
    for (args, n) in [
        (&[][..], 0),
        (&["a", "b"][..], 2),
        (&["a", "b", "c"][..], 3),
    ] {
        let err = get_full_text_tokens(args, "en").unwrap_err();
        let msg = format!("Function requires 1 arguments, but got {n}");
        assert_eq!(err.0, TokErrorEnum::Arguments(msg.clone()));
        assert_eq!(err.to_string(), msg);
    }
}

// NOTE: The Chinese/Japanese/Korean tests were are based on assuming that the
// output is correct (and adding it to the test), with some verification using
// Google translate.

#[test]
fn test_full_text_tokenizer_cjk_chinese() {
    let got = tokens_for_lang("fulltext", "zh", "他是一个薪水很高的商人");
    let want: Vec<Vec<u8>> = [
        "一个", "个薪", "他是", "商人", "很高", "是一", "水很", "的商", "薪水", "高的",
    ]
    .iter()
    .map(|s| enc(IDENT_FULL_TEXT, s))
    .collect();
    assert_eq!(got, want);
    check_sorted_and_unique(&got);
}

#[test]
fn test_full_text_tokenizer_cjk_korean() {
    let got = tokens_for_lang("fulltext", "ko", "그는 큰 급여를 가진 사업가입니다.");
    let want: Vec<Vec<u8>> = ["가진", "그는", "급여를", "사업가입니다", "큰"]
        .iter()
        .map(|s| enc(IDENT_FULL_TEXT, s))
        .collect();
    assert_eq!(got, want);
    check_sorted_and_unique(&got);
}

#[test]
fn test_full_text_tokenizer_cjk_japanese() {
    let got = tokens_for_lang("fulltext", "ja", "彼は大きな給与を持つ実業家です");
    let want: Vec<Vec<u8>> = [
        "きな", "つ実", "です", "な給", "は大", "を持", "与を", "大き", "実業", "家で", "彼は",
        "持つ", "業家", "給与",
    ]
    .iter()
    .map(|s| enc(IDENT_FULL_TEXT, s))
    .collect();
    assert_eq!(got, want);
    check_sorted_and_unique(&got);
}

#[test]
fn test_term_tokenize_cjk_chinese() {
    let got = tokens_for_lang("term", "zh", "第一轮 第二轮 第一轮");
    assert_eq!(
        got,
        vec![enc(IDENT_TERM, "第一轮"), enc(IDENT_TERM, "第二轮")]
    );
    check_sorted_and_unique(&got);
}

#[test]
fn test_ngram_tokenizer() {
    let tokens = tokens_for_lang("ngram", "en", "quick brown fox");
    // Go checks that these are among the tokens; they are all of them, sorted in this port.
    assert_eq!(
        tokens,
        ngrams(&[
            "brown",
            "brown fox",
            "fox",
            "quick",
            "quick brown",
            "quick brown fox"
        ])
    );
    assert_unique(&tokens);
}

#[test]
fn test_ngram_tokenizer_empty() {
    assert!(
        tokens_for_lang("ngram", "en", "").is_empty(),
        "Expected 0 tokens for empty string"
    );
    assert!(
        tokens_for_lang("ngram", "en", "   ").is_empty(),
        "Expected 0 tokens for whitespace only"
    );
}

#[test]
fn test_ngram_tokenizer_single_word() {
    let tokens = tokens_for_lang("ngram", "en", "hello");
    assert_eq!(tokens, ngrams(&["hello"]));
    check_sorted_and_unique(&tokens);
}

#[test]
fn test_ngram_tokenizer_stopwords() {
    let tokens = tokens_for_lang("ngram", "en", "the quick brown fox");
    // "the" is dropped before the shingles are formed, so no shingle holds it.
    assert_eq!(
        tokens,
        ngrams(&[
            "brown",
            "brown fox",
            "fox",
            "quick",
            "quick brown",
            "quick brown fox"
        ])
    );
    assert_unique(&tokens);
}

#[test]
fn test_ngram_tokenizer_stemming() {
    let tokens = tokens_for_lang("ngram", "en", "running quickly");
    assert_eq!(tokens, ngrams(&["quickli", "run", "run quickli"]));
    assert_unique(&tokens);
}

#[test]
fn test_ngram_tokenizer_long_text() {
    let long_text =
        "The quick brown fox jumps over the lazy dog. This is a test sentence with multiple words.";
    let tokens = tokens_for_lang("ngram", "en", long_text);
    // Dgraph's 34 shingles of one to four of the ten words left after the stop words.
    assert_eq!(
        tokens,
        ngrams(&[
            "brown",
            "brown fox",
            "brown fox jump",
            "brown fox jump lazi",
            "dog",
            "dog test",
            "dog test sentenc",
            "dog test sentenc multipl",
            "fox",
            "fox jump",
            "fox jump lazi",
            "fox jump lazi dog",
            "jump",
            "jump lazi",
            "jump lazi dog",
            "jump lazi dog test",
            "lazi",
            "lazi dog",
            "lazi dog test",
            "lazi dog test sentenc",
            "multipl",
            "multipl word",
            "quick",
            "quick brown",
            "quick brown fox",
            "quick brown fox jump",
            "sentenc",
            "sentenc multipl",
            "sentenc multipl word",
            "test",
            "test sentenc",
            "test sentenc multipl",
            "test sentenc multipl word",
            "word",
        ])
    );
    let phrases = [
        "quick",
        "brown",
        "fox",
        "quick brown",
        "brown fox",
        "quick brown fox",
        "sentence multiple words",
        "lazy dog test sentence",
    ];
    for phrase in phrases {
        let expected_tokens = tokens_for_lang("ngram", "en", phrase);
        // For multi-word phrases, find the longest token (likely the full phrase); for single words, the first.
        let expected = if phrase.contains(' ') {
            expected_tokens.iter().max_by_key(|t| t.len()).cloned()
        } else {
            expected_tokens.first().cloned()
        };
        let expected =
            expected.unwrap_or_else(|| panic!("Should find a token for phrase: {phrase}"));
        assert!(
            tokens.contains(&expected),
            "Should contain token for phrase: {phrase}"
        );
    }
    assert_unique(&tokens);
}

#[test]
fn test_ngram_tokenizer_query_tokens() {
    let q = build_ngram_query_tokens(&Value::str("quick brown fox"), "en");
    assert_eq!(q, ngrams(&["quick brown fox"]));
    let q2 = build_ngram_query_tokens(&Value::str("hello"), "en");
    assert_eq!(q2, ngrams(&["hello"]));
    assert!(build_ngram_query_tokens(&Value::str(""), "en").is_empty());
    // A value that is not a string, and one of stop words alone, have no shingles.
    assert!(build_ngram_query_tokens(&Value::Int(1), "en").is_empty());
    assert!(build_ngram_query_tokens(&Value::str("the a"), "en").is_empty());
}

#[test]
fn test_ngram_tokenizer_query_tokens_various_inputs() {
    // Go's cases, with the query tokens Dgraph gives each: three-word shingles of the stemmed words left after the
    // stop words, or one shingle of all of them when fewer remain.
    let cases: [(&str, &str, &[&str]); 5] = [
        (
            "quick brown fox",
            "3 words should generate 1 trigram",
            &["quick brown fox"],
        ),
        (
            "hello world",
            "2 words should generate 1 bigram",
            &["hello world"],
        ),
        ("single", "1 word should generate 1 unigram", &["singl"]),
        (
            "one two three four",
            "4 words should generate 2 trigrams",
            &["on two three", "two three four"],
        ),
        (
            "the quick brown fox jumps",
            "5 words should generate 2 trigrams (after stopword removal)",
            &["brown fox jump", "quick brown fox"],
        ),
    ];
    for (input, description, want) in cases {
        let q = build_ngram_query_tokens(&Value::str(input), "en");
        assert_eq!(q, ngrams(want), "{description}");
    }
}

#[test]
fn test_ngram_tokenizer_language_support() {
    // The port gives the ngram tokenizer its language (ledger 14); these are Go's `GetNGramTokens` for each.
    for (lang, want) in [
        ("en", ["hello", "hello world", "world"]),
        ("es", ["hell", "hell world", "world"]),
        ("fr", ["helo", "helo world", "world"]),
        ("de", ["hello", "hello world", "world"]),
    ] {
        let tokens = tokens_for_lang("ngram", lang, "hello world");
        assert_eq!(tokens, ngrams(&want), "{lang}");
        assert_unique(&tokens);
    }
}

#[test]
fn test_ngram_tokenizer_long_token_hashing() {
    let long_words = "supercalifragilisticexpialidocious antidisestablishmentarianism \
                      pneumonoultramicroscopicsilicovolcanoconiosiss";
    let tokens = tokens_for_lang("ngram", "en", long_words);
    // Dgraph's six tokens: every shingle of 30 bytes or more as its BLAKE2b-256 digest, the 25-byte stem
    // "antidisestablishmentarian" as text.
    let mut want: Vec<Vec<u8>> = [
        "0f00d2dbb71439248c09fa1509c9bb01f3b5f627dbba86300532e80b61b8535226",
        "0f1af1c83c671abc5fff1d7184dd4d71a477830dbf27abe555a05a494103a5bba5",
        "0f620c55701c74e521d339005be167ece563490a72a96171175b08c0668aec8889",
        "0f79ddacd65660729f2fdcac85de047f62fab2deead6ac9c42cc9edab14ebd80d7",
        "0fa9e462d93dde25866ad87dd01c1818b42d1978df5bbe42ce2e02813c51ec5825",
    ]
    .iter()
    .map(|h| unhex(h))
    .collect();
    want.push(enc(IDENT_NGRAM, "antidisestablishmentarian"));
    want.sort();
    assert_eq!(tokens, want);
    assert_unique(&tokens);
    // A 29-byte shingle is kept as text, a 30-byte one hashed.
    assert_eq!(
        get_ngram_tokens("a".repeat(29), "en"),
        vec![enc(IDENT_NGRAM, &"a".repeat(29))]
    );
    let b30 = "b".repeat(30);
    let mut digest = vec![IDENT_NGRAM];
    digest.extend_from_slice(&Blake2b256::digest(b30.as_bytes()));
    assert_eq!(get_ngram_tokens(&b30, "en"), vec![digest]);
}

#[test]
fn test_ngram_tokenizer_non_string_input() {
    let t = get_tokenizer_for_lang(&tokenizer("ngram"), "en");
    assert!(
        build_tokens(&Value::Int(123), &t).unwrap().is_empty(),
        "Expected empty tokens for non-string input"
    );
}

#[test]
fn test_ngram_tokens_with_lang() {
    assert_eq!(
        get_ngram_tokens("quick brown fox", "en"),
        ngrams(&[
            "brown",
            "brown fox",
            "fox",
            "quick",
            "quick brown",
            "quick brown fox"
        ])
    );
    // German stop words and stems, as Go's GetNGramTokens and GetNGramQueryTokens give them for "de".
    assert_eq!(
        get_ngram_tokens("Katzen und Auffassung", "de"),
        ngrams(&["auffassung", "katz", "katz auffassung"])
    );
    assert_eq!(
        get_ngram_query_tokens("Katzen und Auffassung", "de"),
        ngrams(&["katz auffassung"])
    );
    assert!(get_ngram_tokens("", "en").is_empty());
    assert!(get_ngram_query_tokens("", "en").is_empty());
    assert_eq!(
        get_ngram_query_tokens("quick fox", "en"),
        vec![enc(IDENT_NGRAM, "quick fox")]
    );
}

#[test]
fn test_registry_lookups() {
    // Dgraph's registered identifiers; 0x03 is the exact tokenizer's with a language, which is not registered.
    let by_id: [(u8, &str); 17] = [
        (0x01, "term"),
        (0x02, "exact"),
        (0x04, "year"),
        (0x05, "geo"),
        (0x06, "int"),
        (0x07, "float"),
        (0x08, "fulltext"),
        (0x09, "bool"),
        (0x0A, "trigram"),
        (0x0B, "hash"),
        (0x0C, "sha256"),
        (0x0D, "bigfloat"),
        (0x0E, "hnsw"),
        (0x0F, "ngram"),
        (0x41, "month"),
        (0x42, "day"),
        (0x43, "hour"),
    ];
    for id in 0..=u8::MAX {
        let want = by_id.iter().find(|(i, _)| *i == id).map(|(_, n)| *n);
        assert_eq!(get_tokenizer_by_id(id).map(|t| t.name()), want, "{id:#04x}");
    }
    let mut names = TOKENIZER_NAMES.to_vec();
    names.sort();
    names.dedup();
    assert_eq!(names.len(), TOKENIZER_NAMES.len());
    let mut registered: Vec<&str> = by_id.iter().map(|(_, n)| *n).collect();
    registered.sort();
    assert_eq!(names, registered);
    let names = get_tokenizers(&["term", "exact", "term"]).unwrap();
    assert_eq!(
        names.iter().map(|t| t.name()).collect::<Vec<_>>(),
        vec!["term", "exact", "term"]
    );
    assert!(get_tokenizers::<&str>(&[]).unwrap().is_empty());
    for (names, bad) in [(&["term", "nope"][..], "nope"), (&["", "term"][..], "")] {
        let e = get_tokenizers(names).unwrap_err();
        assert_eq!(
            e.0,
            TokErrorEnum::UnknownTokenizer(format!("Invalid tokenizer {bad}"))
        );
    }
    assert!(!tokenizer("geo").is_valid_for(TypeId::BOOL));
    assert!(tokenizer("geo").is_valid_for(TypeId::GEO));
    assert_eq!(
        get_tokenizer_for_lang(&tokenizer("exact"), "de")
            .prefix()
            .unwrap(),
        b"\x03de\x1f".to_vec()
    );
    // A tag that does not parse takes English's base and collator.
    assert_eq!(
        get_tokenizer_for_lang(&tokenizer("exact"), "x!")
            .prefix()
            .unwrap(),
        b"\x03en\x1f".to_vec()
    );
    // No language keeps the tokenizer as it is.
    assert_eq!(
        get_tokenizer_for_lang(&tokenizer("exact"), "").prefix(),
        Some(vec![0x02])
    );
    assert_eq!(tokenizer("exact").prefix().unwrap(), vec![0x02]);
    for name in TOKENIZER_NAMES.iter().filter(|n| **n != "exact") {
        assert!(tokenizer(name).prefix().is_none(), "{name}");
    }
}

#[test]
fn test_encode_tokens() {
    let mut toks = vec![b"a".to_vec(), Vec::new()];
    encode_geo_tokens(&mut toks);
    assert_eq!(toks, vec![b"\x05a".to_vec(), vec![0x05]]);
    encode_regex_tokens(&mut toks);
    assert_eq!(toks, vec![b"\x0a\x05a".to_vec(), vec![0x0a, 0x05]]);
    let mut none: Vec<Vec<u8>> = Vec::new();
    encode_geo_tokens(&mut none);
    encode_regex_tokens(&mut none);
    assert!(none.is_empty());
}

#[derive(Debug, Clone, Deserialize)]
struct TokensCase {
    tokenizer: String,
    lang: String,
    via: String,
    kind: String,
    input: String,
    tokens: Vec<String>,
    sorted: bool,
    error: String,
    /// The port's tokens, where they differ from Dgraph's.
    #[serde(default)]
    fixed: Option<Vec<String>>,
}

fn converted(kind: &str, text: &str) -> Value {
    let to = match kind {
        "bigfloat" => TypeId::BIGFLOAT,
        "datetime" => TypeId::DATETIME,
        _ => TypeId::GEO,
    };
    convert(
        &Val::new(TypeId::STRING, Value::Bytes(text.as_bytes().to_vec())),
        to,
    )
    .unwrap()
    .into_value()
    .unwrap()
}

fn golden_value(c: &TokensCase) -> Value {
    match c.kind.as_str() {
        "string" => Value::Str(untext(&c.input)),
        "int" => Value::Int(c.input.parse().unwrap()),
        "float" => Value::Float(f64::from_bits(u64::from_str_radix(&c.input, 16).unwrap())),
        "bool" => Value::Bool(c.input == "true"),
        "vfloat" => Value::VFloat(vec![1.0, 2.0, 3.0]),
        kind => converted(kind, &c.input),
    }
}

/// The golden ngram index cases whose tokens differ from Dgraph's because the port's `get_tokenizer_for_lang` gives
/// ngram the language (ledger 14), by language and input. The port's result is the case's `fixed` field, which the
/// generator writes as Go's `GetNGramTokens` for the language, the golden `ngram` case of the same input.
const NGRAM_LANG_DEVIATIONS: &[(&str, &str, u32)] = &[
    ("de", "Katzen und Auffassung und Auffassung", 14),
    (
        "de",
        "Die Katzen und die Hunde spielten fröhlich im großen Garten.",
        14,
    ),
    (
        "de",
        "Häuser, Bäume, Straßen und Gärten; ÄÖÜ äöü ß ẞ STRASSE",
        14,
    ),
    (
        "es",
        "deseándoles muchas alegrías a las señoritas y los señores programadores de Dgraph",
        14,
    ),
    (
        "es",
        "Los niños estaban corriendo rápidamente por las calles de las ciudades.",
        14,
    ),
    ("zh", "他是一个薪水很高的商人", 14),
    ("zh", "他是 一个好人", 14),
    ("zh", "第一轮 第二轮 第一轮", 14),
    ("zh", "我爱北京天安门。Dgraph是图数据库, 版本25", 14),
];

/// The ledger entry a golden case deviates by: [`NGRAM_LANG_DEVIATIONS`] for an ngram index case with a language,
/// [`LOWERCASE_DEVIATIONS`] for an analyzed case of the inputs it names.
fn deviation(c: &TokensCase) -> Option<u32> {
    if c.kind != "string" {
        return None;
    }
    let input = untext(&c.input);
    if c.tokenizer == "ngram"
        && c.via == "build"
        && let Some(&(_, _, ledger)) = NGRAM_LANG_DEVIATIONS
            .iter()
            .find(|(lang, i, _)| *lang == c.lang && *i == input)
    {
        return Some(ledger);
    }
    LOWERCASE_DEVIATIONS
        .iter()
        .find(|(i, _)| *i == input)
        .map(|&(_, ledger)| ledger)
}

/// The port's tokens of a golden case.
fn port_tokens(c: &TokensCase) -> Result<Vec<Vec<u8>>, String> {
    let got = match c.via.as_str() {
        "build" => build_tokens(
            &golden_value(c),
            &get_tokenizer_for_lang(&tokenizer(&c.tokenizer), &c.lang),
        )
        .map_err(|e| e.to_string()),
        "ngram" => Ok(get_ngram_tokens(untext(&c.input), &c.lang)),
        _ => Ok(get_ngram_query_tokens(untext(&c.input), &c.lang)),
    };
    got.map(|mut t| {
        if c.sorted {
            t.sort();
        }
        t
    })
}

#[test]
fn test_tokens_golden() {
    let cases: Vec<TokensCase> = cases("tok_tokens");
    assert!(cases.len() > 10_000);
    let mut failures = Vec::new();
    let mut deviating = (Vec::new(), Vec::new());
    for c in &cases {
        let decoded = |t: &[String]| t.iter().map(|t| unhex(t)).collect::<Vec<_>>();
        let want: Result<Vec<Vec<u8>>, String> = match (&c.fixed, deviation(c)) {
            (Some(fixed), Some(ledger)) => {
                assert_ne!(fixed, &c.tokens, "{c:?}: Dgraph differs");
                if ledger == 14 {
                    let go = cases
                        .iter()
                        .find(|g| g.via == "ngram" && g.lang == c.lang && g.input == c.input);
                    assert_eq!(go.map(|g| &g.tokens), Some(fixed), "{c:?}: GetNGramTokens");
                    deviating.0.push((c.lang.clone(), untext(&c.input)));
                } else {
                    deviating.1.push(untext(&c.input));
                }
                Ok(decoded(fixed))
            }
            (None, _) if c.error.is_empty() => Ok(decoded(&c.tokens)),
            (None, _) => Err(c.error.clone()),
            (Some(_), None) => panic!("{c:?}: a fixed result on a case no table names"),
        };
        let got = port_tokens(c);
        if got != want {
            failures.push(format!(
                "{} {:?} via {} of {} {:?}: want {want:?}, got {got:?}",
                c.tokenizer, c.lang, c.via, c.kind, c.input
            ));
        }
    }
    // Every entry deviates on a case of its own; a lowercase input on several tokenizers and languages.
    deviating.1.sort();
    deviating.1.dedup();
    assert_eq!(deviating.0.len(), NGRAM_LANG_DEVIATIONS.len());
    assert_eq!(deviating.1.len(), LOWERCASE_DEVIATIONS.len());
    assert!(
        failures.is_empty(),
        "{} of {} differ:\n{}",
        failures.len(),
        cases.len(),
        failures[..failures.len().min(10)].join("\n")
    );
}

#[derive(Debug, Clone, Deserialize)]
struct RegistryCase {
    name: String,
    found: bool,
    #[serde(rename = "type")]
    typ: String,
    identifier: u8,
    sortable: bool,
    lossy: bool,
    factory: bool,
    lang_identifier: u8,
}

#[test]
fn test_registry_golden() {
    for c in cases::<RegistryCase>("tok_registry") {
        let t = get_tokenizer(&c.name);
        assert_eq!(t.is_some(), c.found, "{}", c.name);
        let Some(t) = t else { continue };
        assert_eq!(t.name(), c.name);
        assert_eq!(t.type_name(), c.typ, "{}", c.name);
        assert_eq!(Some(t.type_id()), TypeId::for_name(&c.typ), "{}", c.name);
        assert_eq!(t.identifier(), c.identifier, "{}", c.name);
        assert_eq!(t.is_sortable(), c.sortable, "{}", c.name);
        assert_eq!(t.is_lossy(), c.lossy, "{}", c.name);
        assert_eq!(t.index_factory().is_some(), c.factory, "{}", c.name);
        assert_eq!(
            get_tokenizer_for_lang(&t, "de").identifier(),
            c.lang_identifier,
            "{}",
            c.name
        );
    }
}

#[test]
fn test_full_text_tokens_of_a_long_run_of_y_do_not_overflow_the_stack() {
    // The Porter stemmer decides whether a `y` is a consonant from the letter before it; go-porterstemmer recurses
    // once per `y` of a run and lives on Go's growable stack, which a thread's fixed stack is not.
    // A suffix that step 2 removes makes it measure the stem, which asks about every `y` of the run.
    let run = format!("{}ational", "y".repeat(100_000));
    let tokens = get_full_text_tokens(&[run.as_str()], "en").unwrap();
    // Dgraph's stem: step 2 makes "ational" "ate", step 4 removes it.
    assert_eq!(tokens, vec![enc(IDENT_FULL_TEXT, &"y".repeat(100_000))]);
    assert_eq!(
        get_full_text_tokens(&["yyyy"], "en").unwrap(),
        vec![b"\x08yyyi".to_vec()]
    );
}

/// A value of each Go type a tokenizer can be handed, with Go's `%T` of it.
fn values_of_every_type() -> Vec<(&'static str, Value)> {
    vec![
        ("string", Value::str("a")),
        ("[]uint8", Value::Bytes(b"a".to_vec())),
        ("int64", Value::Int(1)),
        ("float64", Value::Float(1.5)),
        ("bool", Value::Bool(true)),
        ("time.Time", Value::DateTime(DateTime::from_unix(0, 0))),
        (
            "*geom.Point",
            Value::Geo(Geometry::point(Layout::XY, &[1.0, 2.0]).unwrap()),
        ),
        ("uint64", Value::Uid(1)),
        (
            "big.Float",
            Value::BigFloat(BigFloat::from_f64(1.5, 53).unwrap()),
        ),
        ("[]float32", Value::VFloat(vec![1.0])),
    ]
}

/// Every tokenizer given a value of every type, as Dgraph v25.5.0 answers (a Go program over `tok.BuildTokens`,
/// recovering its panics): the typed tokenizers panic on Go's type assertion, which the port returns as
/// [`TokErrorEnum::WrongValue`] with the panic's text; exact, trigram and hash return their errors; term, fulltext,
/// ngram and sha256 take any value; hnsw ignores it.
#[test]
fn test_tokens_of_values_of_every_type() {
    let int1 = b"\x01\x00\x00\x00\x00\x00\x00\x00\x01";
    // The token each typed tokenizer gives its own type's value of `values_of_every_type`, the epoch for datetime.
    let typed: [(&str, &str, Vec<u8>); 8] = [
        ("bigfloat", "big.Float", [&[0x0d][..], int1].concat()),
        ("int", "int64", [&[0x06][..], int1].concat()),
        ("float", "float64", [&[0x07][..], int1].concat()),
        ("year", "time.Time", b"\x04\x07\xb2".to_vec()),
        ("month", "time.Time", b"\x41\x07\xb2\x00\x01".to_vec()),
        ("day", "time.Time", b"\x42\x07\xb2\x00\x01\x00\x01".to_vec()),
        (
            "hour",
            "time.Time",
            b"\x43\x07\xb2\x00\x01\x00\x01\x00\x00".to_vec(),
        ),
        ("bool", "bool", [&[0x09][..], int1].concat()),
    ];
    for (name, own, token) in &typed {
        let t = tokenizer(name);
        for (go, v) in values_of_every_type() {
            let got = build_tokens(&v, &t);
            if go == *own {
                assert_eq!(got, Ok(vec![token.clone()]), "{name} of {go}");
            } else {
                let e = got.expect_err(name);
                assert_eq!(
                    e.0,
                    TokErrorEnum::WrongValue(format!(
                        "interface conversion: interface {{}} is {go}, not {own}"
                    )),
                    "{name} of {go}"
                );
            }
        }
    }
    // geom.T is an interface, whose failed assertion Go words by the first method the value lacks.
    for (go, v) in values_of_every_type() {
        let got = build_tokens(&v, &tokenizer("geo"));
        if go == "*geom.Point" {
            // Go's tokens of POINT(1 2), in its order: the parent cells, then the cover cell.
            let want: Vec<Vec<u8>> = [
                "p/1004",
                "p/1003",
                "p/10034",
                "p/10037",
                "p/10036c",
                "p/10036f",
                "p/10036e4",
                "p/10036e1",
                "p/10036e14",
                "p/10036e17",
                "p/10036e17c",
                "p/10036e17d",
                "c/10036e17d",
            ]
            .iter()
            .map(|s| enc(0x05, s))
            .collect();
            assert_eq!(got, Ok(want), "geo of {go}");
        } else {
            assert_eq!(
                got.unwrap_err().to_string(),
                format!("interface conversion: {go} is not geom.T: missing method Bounds"),
                "geo of {go}"
            );
        }
    }
    let refused = [
        ("exact", "Exact indices only supported for string types"),
        ("trigram", "Trigram indices only supported for string types"),
        ("hash", "Hash tokenizer only supported for string types"),
    ];
    for (name, msg) in refused {
        for (go, v) in values_of_every_type().into_iter().skip(1) {
            let e = build_tokens(&v, &tokenizer(name)).expect_err(name);
            assert_eq!(e.0, TokErrorEnum::WrongValue(msg.into()), "{name} of {go}");
        }
    }
    for (go, v) in values_of_every_type().into_iter().skip(1) {
        assert_eq!(
            build_tokens(&v, &tokenizer("term")),
            Ok(vec![vec![IDENT_TERM]]),
            "term of {go}"
        );
        for name in ["fulltext", "ngram", "sha256"] {
            assert_eq!(
                build_tokens(&v, &tokenizer(name)),
                Ok(Vec::new()),
                "{name} of {go}"
            );
        }
    }
    for (go, v) in values_of_every_type() {
        assert_eq!(
            build_tokens(&v, &tokenizer("hnsw")),
            Ok(vec![b"\x0efloat".to_vec()]),
            "hnsw of {go}"
        );
    }
}
