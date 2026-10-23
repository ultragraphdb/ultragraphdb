/*
 * Copyright 2013 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from golang.org/x/text v0.41.0, language/parse_test.go (TestErrors) and language/language_test.go
 * (TestBase, TestIsRoot, TestString). Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_core_text::{Confidence, LanguageErrorEnum, Tag};

#[test]
fn errors() {
    let invalid = |s: &str| LanguageErrorEnum::Value(s.as_bytes().to_vec());
    let tests = [
        // invalid subtags.
        ("ac", invalid("ac")),
        ("AC", invalid("ac")),
        ("aa-Uuuu", invalid("Uuuu")),
        ("aa-AB", invalid("AB")),
        // ill-formed wins over invalid.
        ("ac-u", LanguageErrorEnum::Syntax),
        ("ac-u-ca", invalid("ac")),
        ("ac-u-ca-co-pinyin", invalid("ac")),
        ("noob", LanguageErrorEnum::Syntax),
    ];
    for (input, want) in tests {
        let err = Tag::parse(input).unwrap_err();
        assert_eq!(err.0, want, "{input}");
    }
}

#[test]
fn base() {
    let tests = [
        ("und", "en", Confidence::Low),
        ("x-abc", "und", Confidence::No),
        ("en", "en", Confidence::Exact),
        ("und-Cyrl", "ru", Confidence::High),
        // If a region is not included, the official language should be English.
        ("und-US", "en", Confidence::High),
    ];
    for (i, (loc, lang, conf)) in tests.into_iter().enumerate() {
        let tag = Tag::parse(loc).unwrap_or_default();
        let (base, c) = tag.base();
        assert_eq!(base.to_string(), lang, "{i}: language");
        assert_eq!(c, conf, "{i}: confidence");
    }
}

#[test]
fn is_root() {
    for (input, root) in [
        ("und", true),
        ("und-u-co-phonebk", false),
        ("und-x-a", false),
        ("en", false),
    ] {
        let tag = Tag::parse(input).unwrap();
        assert_eq!(tag == Tag::default(), root, "{input}");
    }
}

#[test]
fn string() {
    let tests = [
        ("und", "und"),
        ("en", "en"),
        ("en-US", "en-US"),
        ("en_us", "en-US"),
        ("ES-419", "es-419"),
        ("zh_hant_tw", "zh-Hant-TW"),
        ("sh", "sr-Latn"),
        ("en-u-cu-xua-co-phonebk", "en-u-co-phonebk-cu-xua"),
        ("x-a-b-c-d", "x-a-b-c-d"),
    ];
    for (input, want) in tests {
        assert_eq!(Tag::parse(input).unwrap().to_string(), want, "{input}");
    }
}
