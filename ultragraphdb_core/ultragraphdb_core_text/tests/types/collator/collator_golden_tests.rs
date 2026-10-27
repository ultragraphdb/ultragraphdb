/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::cmp::Ordering;

use serde::Deserialize;
use ultragraphdb_core_text::{Collator, Tag};
use ultragraphdb_tool_bdd::{compare_bytes, load_golden, testdata_dir};

#[derive(Debug, Deserialize)]
struct KeyCase {
    lang: String,
    text: String,
    parse_ok: bool,
    key: String,
    lang_base: String,
}

/// A key case whose text, in hex, may hold invalid UTF-8.
#[derive(Debug, Deserialize)]
struct InvalidKeyCase {
    lang: String,
    text: String,
    parse_ok: bool,
    key: String,
    lang_base: String,
}

#[derive(Debug, Deserialize)]
struct CompareCase {
    lang: String,
    texts: Vec<String>,
    compare: Vec<String>,
    sorted: Vec<String>,
}

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}

/// The collator Dgraph's exact tokenizer builds: the tag's, or English's when the tag does not parse.
fn dgraph_collator(lang: &str) -> (Collator, bool) {
    match Tag::parse(lang) {
        Ok(tag) => (Collator::new(&tag), true),
        Err(_) => (Collator::new(&Tag::parse("en").unwrap()), false),
    }
}

/// Every key equals Go's `KeyFromString`, byte for byte, and the exact token's base language Dgraph's.
#[test]
fn keys_match_go() {
    check_keys("text_collate_keys", 20_000);
}

/// The same for strings drawn at random from runes that reach the collator's rarer paths.
#[test]
fn random_keys_match_go() {
    check_keys("text_collate_random", 10_000);
}

fn check_keys(name: &str, min_cases: usize) {
    let path = testdata_dir().join(format!("golden/{name}.json"));
    let golden = load_golden::<KeyCase>(&path);
    assert!(golden.cases().len() > min_cases);
    let mut failures = Vec::new();
    let mut current: Option<(String, Collator)> = None;
    for c in golden.cases() {
        if current.as_ref().is_none_or(|(l, _)| *l != c.lang) {
            let (collator, ok) = dgraph_collator(&c.lang);
            assert_eq!(ok, c.parse_ok, "{:?} parses", c.lang);
            current = Some((c.lang.clone(), collator));
        }
        let (_, collator) = current.as_ref().unwrap();
        let key = collator.key(&c.text);
        if let Err(e) = compare_bytes(&format!("{} {:?}", c.lang, c.text), &unhex(&c.key), &key) {
            failures.push(e);
        }
        // The exact token: base language, 0x1f, key.
        if !c.lang.is_empty() {
            let base =
                ultragraphdb_core_text::language_base(&c.lang).unwrap_or_else(|| "en".to_string());
            assert_eq!(base, c.lang_base, "{:?}", c.lang);
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} keys differ:\n{}",
        failures.len(),
        golden.cases().len(),
        failures[..failures.len().min(50)].join("\n")
    );
}

/// Every comparison equals Go's `CompareString`, and sorting by it gives Go's order.
#[test]
fn comparisons_match_go() {
    let golden =
        load_golden::<CompareCase>(&testdata_dir().join("golden/text_collate_compare.json"));
    assert!(!golden.cases().is_empty());
    let mut failures = Vec::new();
    for c in golden.cases() {
        let collator = Collator::new(&Tag::parse(&c.lang).unwrap());
        for (i, a) in c.texts.iter().enumerate() {
            let row: String = c
                .texts
                .iter()
                .map(|b| match collator.compare(a, b) {
                    Ordering::Less => '<',
                    Ordering::Equal => '=',
                    Ordering::Greater => '>',
                })
                .collect();
            if row != c.compare[i] {
                failures.push(format!(
                    "{} {a:?}: got {row}, want {}",
                    c.lang, c.compare[i]
                ));
            }
        }
        let mut sorted = c.texts.clone();
        sorted.sort_by(|a, b| collator.compare(a, b));
        if sorted != c.sorted {
            failures.push(format!(
                "{}: sorted {sorted:?}, want {:?}",
                c.lang, c.sorted
            ));
        }
    }
    assert!(
        failures.is_empty(),
        "{} differ:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// Keys of Go strings holding invalid UTF-8 equal Go's `KeyFromString` (and `Key`) over the same bytes.
#[test]
fn invalid_utf8_keys_match_go() {
    let path = testdata_dir().join("golden/text_collate_invalid.json");
    let golden = load_golden::<InvalidKeyCase>(&path);
    assert!(golden.cases().len() > 20_000);
    let mut invalid = 0;
    let mut failures = Vec::new();
    let mut current: Option<(String, Collator)> = None;
    for c in golden.cases() {
        if current.as_ref().is_none_or(|(l, _)| *l != c.lang) {
            let (collator, ok) = dgraph_collator(&c.lang);
            assert_eq!(ok, c.parse_ok, "{:?} parses", c.lang);
            let base =
                ultragraphdb_core_text::language_base(&c.lang).unwrap_or_else(|| "en".to_string());
            assert_eq!(base, c.lang_base, "{:?}", c.lang);
            current = Some((c.lang.clone(), collator));
        }
        let (_, collator) = current.as_ref().unwrap();
        let text = unhex(&c.text);
        if std::str::from_utf8(&text).is_err() {
            invalid += 1;
        }
        let key = collator.key_bytes(&text);
        if let Err(e) = compare_bytes(&format!("{} {}", c.lang, c.text), &unhex(&c.key), &key) {
            failures.push(e);
        }
    }
    assert!(invalid > 20_000, "{invalid} texts hold invalid UTF-8");
    assert!(
        failures.is_empty(),
        "{} of {} keys differ:\n{}",
        failures.len(),
        golden.cases().len(),
        failures[..failures.len().min(50)].join("\n")
    );
}

/// Comparisons of Go strings holding invalid UTF-8 equal Go's `CompareString` (and `Compare`), and sorting by them
/// gives Go's order.
#[test]
fn invalid_utf8_comparisons_match_go() {
    let golden = load_golden::<CompareCase>(
        &testdata_dir().join("golden/text_collate_invalid_compare.json"),
    );
    assert!(golden.cases().len() >= 30);
    let mut failures = Vec::new();
    for c in golden.cases() {
        let collator = Collator::new(&Tag::parse(&c.lang).unwrap());
        let texts: Vec<Vec<u8>> = c.texts.iter().map(|t| unhex(t)).collect();
        for (i, a) in texts.iter().enumerate() {
            let row: String = texts
                .iter()
                .map(|b| match collator.compare_bytes(a, b) {
                    Ordering::Less => '<',
                    Ordering::Equal => '=',
                    Ordering::Greater => '>',
                })
                .collect();
            if row != c.compare[i] {
                failures.push(format!(
                    "{} {}: got {row}, want {}",
                    c.lang, c.texts[i], c.compare[i]
                ));
            }
        }
        let mut sorted = texts.clone();
        sorted.sort_by(|a, b| collator.compare_bytes(a, b));
        let want: Vec<Vec<u8>> = c.sorted.iter().map(|t| unhex(t)).collect();
        if sorted != want {
            failures.push(format!("{}: sorted differently", c.lang));
        }
    }
    assert!(
        failures.is_empty(),
        "{} differ:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
