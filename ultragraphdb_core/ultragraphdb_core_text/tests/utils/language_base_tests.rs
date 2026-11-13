/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_text::language_base;

/// A tag's base is its language subtag, whatever script, region or variant follows it, in lower case.
#[test]
fn test_a_tags_base_is_its_language_subtag() {
    for (tag, base) in [
        ("en", "en"),
        ("en-US", "en"),
        ("EN-us", "en"),
        ("zh-Hant-TW", "zh"),
        ("de-u-co-phonebk", "de"),
        ("sr-Latn", "sr"),
    ] {
        assert_eq!(language_base(tag).as_deref(), Some(base), "{tag}");
    }
}

/// A tag naming no language, and a text that is no tag, have no base.
#[test]
fn test_no_language_has_no_base() {
    for tag in ["und", "", "x!", "x-klingon", "toolonglanguage", "en--US"] {
        assert_eq!(language_base(tag), None, "{tag:?}");
    }
}
