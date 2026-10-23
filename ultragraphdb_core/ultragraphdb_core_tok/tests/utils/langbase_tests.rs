/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, tok/langbase_test.go. Changed for ultragraphdb: translated to Rust; the golden
 * vectors of tools/golden/tok.go added.
 * Copyright (c) 2026. Marvin Hansen.
 */

use serde::Deserialize;
use tracing::Level;
use ultragraphdb_core_tok::lang_base;
use ultragraphdb_store_sim::LogCapture;

use crate::golden::cases;

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
        assert_eq!(lang_base(input), out, "{input}");
    }
}

#[derive(Debug, Clone, Deserialize)]
struct LangBaseCase {
    #[serde(rename = "in")]
    input: String,
    out: String,
}

#[test]
fn test_lang_base_golden() {
    let cases: Vec<LangBaseCase> = cases("tok_lang_base");
    assert!(cases.len() > 90);
    for c in cases {
        assert_eq!(lang_base(&c.input), c.out, "{:?}", c.input);
        // A second lookup reads the cache.
        assert_eq!(lang_base(&c.input), c.out, "{:?} cached", c.input);
    }
}

/// The events Dgraph's `LangBase` logs: an error for a tag that does not parse, and a warning for every tag
/// reverted to English, once per tag while it is cached. The tags are used by no other test, so the cache, shared by
/// every thread, is empty for them.
#[test]
fn test_lang_base_logs_a_tag_reverted_to_english() {
    let capture = LogCapture::new();
    tracing::subscriber::with_default(capture.subscriber(), || {
        assert_eq!(lang_base("x-ugdbtel"), "en");
        assert_eq!(lang_base("not a tag!"), "en");
        assert_eq!(lang_base("not a tag!"), "en");
        assert_eq!(lang_base("de-AT"), "de");
    });
    let captured = capture.events();
    let seen: Vec<(Level, Option<&str>, Option<&str>)> = captured
        .iter()
        .map(|e| (e.level(), e.message(), e.field("lang")))
        .collect();
    assert_eq!(
        seen,
        vec![
            (
                Level::WARN,
                Some("Unable to find lang. Reverting to English."),
                Some("x-ugdbtel")
            ),
            (
                Level::ERROR,
                Some("While trying to parse lang"),
                Some("not a tag!")
            ),
            (
                Level::WARN,
                Some("Unable to find lang. Reverting to English."),
                Some("not a tag!")
            ),
        ]
    );
    assert!(
        captured[1].field("error").is_some_and(|e| !e.is_empty()),
        "{:?}",
        captured[1]
    );
}
