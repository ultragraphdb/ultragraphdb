/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, tok/tok_test.go. Changed for ultragraphdb: translated to Rust; the golden vectors of
 * tools/golden/tok.go added.
 * Copyright (c) 2026. Marvin Hansen.
 */

use serde::Deserialize;
use ultragraphdb_core_tok::{
    IDENT_FULL_TEXT, IDENT_NGRAM, IDENT_TERM, Tokenizer, build_ngram_query_tokens, build_tokens,
    encode_geo_tokens, encode_regex_tokens, get_full_text_tokens, get_ngram_query_tokens,
    get_ngram_tokens, get_tokenizer, get_tokenizer_by_id, get_tokenizer_for_lang, get_tokenizers,
};
use ultragraphdb_core_types::{DateTime, TypeId, Val, Value, convert};

use crate::golden::{cases, unhex};

fn enc(id: u8, s: &str) -> Vec<u8> {
    let mut v = vec![id];
    v.extend_from_slice(s.as_bytes());
    v
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
    for (name, len) in [
        ("hour", 1 + 2 * 4),
        ("day", 1 + 2 * 3),
        ("month", 1 + 2 * 2),
        ("year", 1 + 2),
    ] {
        let tokens = build_tokens(&Value::DateTime(dt), &tokenizer(name)).unwrap();
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].len(), len, "{name}");
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
    let tokens = get_full_text_tokens(&["Quick brown fox"], "en").unwrap();
    assert_eq!(tokens.len(), 3);
}

#[test]
fn test_get_full_text_tokens_invalid_lang() {
    let tokens = get_full_text_tokens(&["Quick brown fox"], "xxx_such_language").unwrap();
    assert_eq!(tokens.len(), 3);
}

#[test]
fn test_get_full_text_tokens_argument_count() {
    let err = get_full_text_tokens::<&str>(&[], "en").unwrap_err();
    assert_eq!(err.to_string(), "Function requires 1 arguments, but got 0");
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
    assert!(!tokens.is_empty());
    for expected in [
        "quick",
        "brown",
        "fox",
        "quick brown",
        "brown fox",
        "quick brown fox",
    ] {
        assert!(
            tokens.contains(&enc(IDENT_NGRAM, expected)),
            "Expected token {expected} not found"
        );
    }
    // ngram tokens are not guaranteed to be sorted, just check uniqueness
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
    assert!(tokens.contains(&enc(IDENT_NGRAM, "hello")));
    check_sorted_and_unique(&tokens);
}

#[test]
fn test_ngram_tokenizer_stopwords() {
    let tokens = tokens_for_lang("ngram", "en", "the quick brown fox");
    assert!(!tokens.is_empty());
    assert!(!tokens.contains(&enc(IDENT_NGRAM, "the")));
    assert!(!tokens.contains(&enc(IDENT_NGRAM, "the quick")));
    for t in ["quick", "brown", "fox"] {
        assert!(tokens.contains(&enc(IDENT_NGRAM, t)));
    }
    assert_unique(&tokens);
}

#[test]
fn test_ngram_tokenizer_stemming() {
    let tokens = tokens_for_lang("ngram", "en", "running quickly");
    for t in ["run", "quickli", "run quickli"] {
        assert!(
            tokens.contains(&enc(IDENT_NGRAM, t)),
            "Expected token {t} not found"
        );
    }
    assert_unique(&tokens);
}

#[test]
fn test_ngram_tokenizer_long_text() {
    let long_text =
        "The quick brown fox jumps over the lazy dog. This is a test sentence with multiple words.";
    let tokens = tokens_for_lang("ngram", "en", long_text);
    assert!(tokens.len() > 10);
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
    assert!(q.contains(&enc(IDENT_NGRAM, "quick brown fox")));
    let q2 = build_ngram_query_tokens(&Value::str("hello"), "en");
    assert!(q2.contains(&enc(IDENT_NGRAM, "hello")));
    assert!(build_ngram_query_tokens(&Value::str(""), "en").is_empty());
}

#[test]
fn test_ngram_tokenizer_query_tokens_various_inputs() {
    let cases: [(&str, &str, usize, &[&str]); 5] = [
        (
            "quick brown fox",
            "3 words should generate 1 trigram",
            1,
            &["quick brown fox"],
        ),
        (
            "hello world",
            "2 words should generate 1 bigram",
            1,
            &["hello world"],
        ),
        ("single", "1 word should generate 1 unigram", 1, &["single"]),
        (
            "one two three four",
            "4 words should generate 2 trigrams",
            2,
            &["one two three", "two three four"],
        ),
        (
            "the quick brown fox jumps",
            "5 words should generate 2 trigrams (after stopword removal)",
            2,
            &["quick brown fox", "brown fox jump"],
        ),
    ];
    let ngram = get_tokenizer_for_lang(&tokenizer("ngram"), "en");
    for (input, description, n, contains) in cases {
        let q = build_ngram_query_tokens(&Value::str(input), "en");
        assert_eq!(q.len(), n, "{description}");
        for expected in contains {
            let stemmed = ngram.tokens(&Value::str(*expected)).unwrap();
            if !stemmed.is_empty() {
                let found = stemmed.iter().any(|t| {
                    q.contains(&{
                        let mut e = vec![IDENT_NGRAM];
                        e.extend_from_slice(t);
                        e
                    })
                });
                assert!(
                    found,
                    "Expected to find stemmed version of '{expected}' in query tokens"
                );
            }
        }
    }
}

#[test]
fn test_ngram_tokenizer_language_support() {
    for lang in ["en", "es", "fr", "de"] {
        let tokens = tokens_for_lang("ngram", lang, "hello world");
        assert!(
            !tokens.is_empty(),
            "No tokens generated for language: {lang}"
        );
        assert_unique(&tokens);
    }
}

#[test]
fn test_ngram_tokenizer_long_token_hashing() {
    let long_words = "supercalifragilisticexpialidocious antidisestablishmentarianism \
                      pneumonoultramicroscopicsilicovolcanoconiosiss";
    let tokens = tokens_for_lang("ngram", "en", long_words);
    assert!(!tokens.is_empty());
    assert_unique(&tokens);
    assert!(
        tokens.iter().all(|t| t[0] == IDENT_NGRAM),
        "Token should start with correct identifier"
    );
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
    assert!(get_ngram_tokens("quick brown fox", "en").contains(&enc(IDENT_NGRAM, "quick brown")));
    assert_eq!(
        get_ngram_query_tokens("quick fox", "en"),
        vec![enc(IDENT_NGRAM, "quick fox")]
    );
}

#[test]
fn test_registry_lookups() {
    assert_eq!(get_tokenizer_by_id(IDENT_TERM).unwrap().name(), "term");
    assert!(get_tokenizer_by_id(0x03).is_none());
    assert_eq!(get_tokenizers(&["term", "exact"]).unwrap().len(), 2);
    assert_eq!(
        get_tokenizers(&["term", "nope"]).unwrap_err().to_string(),
        "Invalid tokenizer nope"
    );
    assert!(!tokenizer("geo").is_valid_for(TypeId::BOOL));
    assert!(tokenizer("geo").is_valid_for(TypeId::GEO));
    assert_eq!(
        get_tokenizer_for_lang(&tokenizer("exact"), "de")
            .prefix()
            .unwrap(),
        b"\x03de\x1f".to_vec()
    );
    assert_eq!(tokenizer("exact").prefix().unwrap(), vec![0x02]);
    assert!(tokenizer("term").prefix().is_none());
}

#[test]
fn test_encode_tokens() {
    let mut toks = vec![b"a".to_vec()];
    encode_geo_tokens(&mut toks);
    assert_eq!(toks, vec![b"\x05a".to_vec()]);
    encode_regex_tokens(&mut toks);
    assert_eq!(toks, vec![b"\x0a\x05a".to_vec()]);
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
        "string" => Value::Str(unhex(&c.input)),
        "int" => Value::Int(c.input.parse().unwrap()),
        "float" => Value::Float(f64::from_bits(u64::from_str_radix(&c.input, 16).unwrap())),
        "bool" => Value::Bool(c.input == "true"),
        "vfloat" => Value::VFloat(vec![1.0, 2.0, 3.0]),
        kind => converted(kind, &c.input),
    }
}

#[test]
fn test_tokens_golden() {
    let cases: Vec<TokensCase> = cases("tok_tokens");
    assert!(cases.len() > 10_000);
    let mut failures = Vec::new();
    let mut invalid_exact = 0;
    for c in &cases {
        let v = golden_value(c);
        let got: Result<Vec<Vec<u8>>, String> = match c.via.as_str() {
            "build" => build_tokens(
                &v,
                &get_tokenizer_for_lang(&tokenizer(&c.tokenizer), &c.lang),
            )
            .map_err(|e| e.to_string()),
            "ngram" => Ok(get_ngram_tokens(unhex(&c.input), &c.lang)),
            _ => Ok(get_ngram_query_tokens(unhex(&c.input), &c.lang)),
        };
        let got = got.map(|mut t| {
            if c.sorted {
                t.sort();
            }
            t
        });
        let want: Result<Vec<Vec<u8>>, String> = if c.error.is_empty() {
            Ok(c.tokens.iter().map(|t| unhex(t)).collect())
        } else {
            Err(c.error.clone())
        };
        // An exact token with a language keys the value's bytes, which may be invalid UTF-8; those cases are counted.
        if c.tokenizer == "exact"
            && !c.lang.is_empty()
            && c.kind == "string"
            && std::str::from_utf8(&unhex(&c.input)).is_err()
        {
            invalid_exact += 1;
        }
        if got != want {
            failures.push(format!(
                "{} {:?} via {} of {} {:?}: want {want:?}, got {got:?}",
                c.tokenizer,
                c.lang,
                c.via,
                c.kind,
                if c.kind == "string" {
                    String::from_utf8_lossy(&unhex(&c.input)).into_owned()
                } else {
                    c.input.clone()
                }
            ));
        }
    }
    assert_eq!(invalid_exact, 160);
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
