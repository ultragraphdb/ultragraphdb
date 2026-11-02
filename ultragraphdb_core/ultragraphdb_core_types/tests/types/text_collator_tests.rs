/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::cmp::Ordering;

use ultragraphdb_core_types::TextCollator;
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_traits_collator::Collator;

#[test]
fn test_text_collator_refuses_what_go_refuses() -> TestResult {
    // language.Parse refuses a malformed tag and a well-formed unknown one.
    for lang in ["xx-invalid-tag-!", "xx", ""] {
        assert!(TextCollator.language(lang).is_none(), "{lang:?}");
    }
    // Go's collate.New(tag).CompareString of ("a", "B"), ("B", "a"), ("e", "é") and ("ä", "b") under each tag
    // language.Parse accepts.
    let cases = [
        ("en_US", [-1, 1, -1, -1]),
        ("und", [-1, 1, -1, -1]),
        ("EN", [-1, 1, -1, -1]),
        ("de", [-1, 1, -1, -1]),
        ("zh-Hant-TW", [-1, 1, 1, -1]),
        ("en-u-ka-shifted", [0, 0, 0, 0]),
    ];
    for (lang, want) in cases {
        let c = TextCollator
            .language(lang)
            .ok_or_else(|| format!("{lang} is a tag Go accepts"))
            .context("a tag Go accepts")?;
        let got = [("a", "B"), ("B", "a"), ("e", "é"), ("ä", "b")]
            .map(|(a, b)| TextCollator.compare(&c, a, b) as i8);
        assert_eq!(got, want, "{lang}");
    }
    Ok(())
}

#[test]
fn test_text_collator_compares_as_compare_string() -> TestResult {
    let de = TextCollator
        .language("de")
        .context("TextCollator.language(\"de\")")?;
    assert_eq!(
        TextCollator.compare(&de, "öffnen", "zumachen"),
        Ordering::Less
    );
    let sv = TextCollator
        .language("sv")
        .context("TextCollator.language(\"sv\")")?;
    assert_eq!(TextCollator.compare(&sv, "öppna", "zon"), Ordering::Greater);
    // CompareString, not a comparison of keys: with -u-ka-shifted Go skips the primary level.
    let shifted = TextCollator
        .language("en-u-ka-shifted")
        .context("TextCollator.language(\"en-u-ka-shifted\")")?;
    assert_eq!(TextCollator.compare(&shifted, "a", "b"), Ordering::Equal);
    // Strings that collate equal compare by their bytes under -u-ks-identic.
    let identic = TextCollator
        .language("en-u-ks-identic")
        .context("TextCollator.language(\"en-u-ks-identic\")")?;
    assert_eq!(
        TextCollator.compare(&identic, "a\u{301}", "\u{e1}"),
        "a\u{301}".cmp("\u{e1}")
    );
    Ok(())
}

#[test]
fn test_text_collator_compares_bytes_as_go_compares_strings() -> TestResult {
    let en = TextCollator
        .language("en")
        .context("TextCollator.language(\"en\")")?;
    // An invalid byte weighs as U+FFFD's implicit weight, above U+FFFD's own.
    assert_eq!(
        TextCollator.compare_bytes(&en, b"\xff", "\u{fffd}".as_bytes()),
        Ordering::Greater
    );
    assert_eq!(
        TextCollator.compare_bytes(&en, b"\xfe", b"\xff"),
        Ordering::Equal
    );
    assert_eq!(
        TextCollator.compare(&en, "a", "b"),
        TextCollator.compare_bytes(&en, b"a", b"b")
    );
    Ok(())
}
