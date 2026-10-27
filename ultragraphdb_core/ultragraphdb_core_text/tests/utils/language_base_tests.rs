/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, tok/langbase_test.go. Changed for ultragraphdb: translated to Rust, with Dgraph's
 * fallback to English applied to language_base, and the golden vectors of Dgraph's tok.LangBase.
 * Copyright (c) 2026. Marvin Hansen.
 */

use serde::Deserialize;
use ultragraphdb_core_text::language_base;
use ultragraphdb_tool_bdd::{load_golden, testdata_dir};

/// Dgraph's `tok.LangBase`, without its cache.
fn lang_base(lang: &str) -> String {
    language_base(lang).unwrap_or_else(|| "en".to_string())
}

#[test]
fn test_lang_base() {
    let tests = [
        ("zh", "zh"),
        ("zh-hant", "zh"),
        ("zh-hans", "zh"),
        ("es-es", "es"),
        ("ar-001", "ar"),
        ("ca-ES-valencia", "ca"),
        ("ca-ES-valencia-u-va-posix", "ca"),
        ("ca-ES-valencia-u-co-phonebk", "ca"),
        ("ca-ES-valencia-u-co-phonebk-va-posix", "ca"),
        ("x-klingon", "en"),
        ("en-US", "en"),
        ("en-US-u-va-posix", "en"),
        ("en", "en"),
        ("en-u-co-phonebk", "en"),
        ("en-001", "en"),
        ("sh", "sr"),
        ("en-GB-u-rg-uszzzz", "en"),
        ("en-GB-u-rg-uszzzz-va-posix", "en"),
        ("en-GB-u-co-phonebk-rg-uszzzz", "en"),
        ("en-GB-u-co-phonebk-rg-uszz", "en"),
        ("", "en"),
        ("no_such_language", "en"),
        ("xxx_such_language", "en"),
    ];
    for (input, out) in tests {
        assert_eq!(lang_base(input), out, "{input:?}");
    }
}

/// The spec's scenario "Language tags reduce" (tokenizers), owned by ultragraphdb_core_tok.
#[test]
fn language_tags_reduce() {
    let got: Vec<String> = ["en-US", "zh-Hant", "sh", "und", "x!"]
        .iter()
        .map(|t| lang_base(t))
        .collect();
    assert_eq!(got, ["en", "zh", "sr", "en", "en"]);
}

#[derive(Debug, Deserialize)]
struct LangBaseCase {
    tag: String,
    lang_base: String,
}

#[test]
fn lang_base_matches_dgraph() {
    check_file("text_lang_base");
}

#[test]
fn random_lang_base_matches_dgraph() {
    check_file("text_lang_random");
}

fn check_file(name: &str) {
    let path = testdata_dir().join(format!("golden/{name}.json"));
    let golden = load_golden::<LangBaseCase>(&path);
    let failures: Vec<String> = golden
        .cases()
        .iter()
        .filter(|c| lang_base(&c.tag) != c.lang_base)
        .map(|c| {
            format!(
                "{:?}: got {:?}, want {:?}",
                c.tag,
                lang_base(&c.tag),
                c.lang_base
            )
        })
        .collect();
    assert!(
        failures.is_empty(),
        "{} cases differ:\n{}",
        failures.len(),
        failures.join("\n")
    );
}
