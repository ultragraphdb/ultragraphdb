/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use tracing::Level;
use ultragraphdb_core_tok::lang_base;
use ultragraphdb_store_sim::LogCapture;

/// A tag's base is its language subtag; no tag, a tag naming no language and a text that is no tag give English.
#[test]
fn test_lang_base() {
    let tests = [
        ("zh", "zh"),
        ("zh-hant", "zh"),
        ("zh-hans", "zh"),
        ("es-es", "es"),
        ("ar-001", "ar"),
        ("ca-ES-valencia", "ca"),
        ("ca-ES-valencia-u-co-phonebk", "ca"),
        ("en-US", "en"),
        ("en", "en"),
        ("en-u-co-phonebk", "en"),
        ("en-001", "en"),
        ("sh", "sh"),
        ("fr-x-ugdb", "fr"),
        ("x-klingon", "en"),
        ("und", "en"),
        ("", "en"),
        ("no_such_language", "en"),
    ];
    for (input, out) in tests {
        assert_eq!(lang_base(input), out, "{input}");
        // A second lookup reads the cache.
        assert_eq!(lang_base(input), out, "{input} cached");
    }
}

/// A tag reverted to English is logged as a warning, once while it is cached. The tags are used by no other test, so
/// the cache, shared by every thread, is empty for them.
#[test]
fn test_lang_base_logs_a_tag_reverted_to_english() {
    let capture = LogCapture::new();
    capture.capture(|| {
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
    const REVERT: Option<&str> = Some("Unable to find lang. Reverting to English.");
    assert_eq!(
        seen,
        vec![
            (Level::WARN, REVERT, Some("x-ugdbtel")),
            (Level::WARN, REVERT, Some("not a tag!")),
        ]
    );
}
