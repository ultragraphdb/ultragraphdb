/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::cmp::Ordering;

use ultragraphdb_core_types::TextCollator;
use ultragraphdb_tool_bdd::{Context, TestResult};
use ultragraphdb_traits_collator::Collator;

/// A text that is no language tag has no collation, and the sort compares bytes; a well-formed tag has one, a
/// language CLDR does not tailor collating by the root order.
#[test]
fn test_text_collator_refuses_what_is_no_tag() {
    for lang in ["xx-invalid-tag-!", "", "en--US"] {
        assert!(TextCollator.language(lang).is_none(), "{lang:?}");
    }
    for lang in ["en-US", "und", "EN", "de", "zh-Hant-TW", "xx"] {
        assert!(TextCollator.language(lang).is_some(), "{lang:?}");
    }
}

/// A language's collation orders its letters: German sorts `ö` with `o`, Swedish after `z`.
#[test]
fn test_text_collator_compares_in_the_languages_order() -> TestResult {
    let de = TextCollator.language("de").context("German")?;
    assert_eq!(
        TextCollator.compare(&de, "öffnen", "zumachen"),
        Ordering::Less
    );
    let sv = TextCollator.language("sv").context("Swedish")?;
    assert_eq!(TextCollator.compare(&sv, "öppna", "zon"), Ordering::Greater);
    let en = TextCollator.language("en").context("English")?;
    assert_eq!(TextCollator.compare(&en, "a", "B"), Ordering::Less);
    assert_eq!(TextCollator.compare(&en, "e", "é"), Ordering::Less);
    Ok(())
}

/// Bytes compare as their strings do, each invalid UTF-8 sequence as U+FFFD.
#[test]
fn test_text_collator_compares_bytes_with_invalid_utf8_as_the_replacement_character() -> TestResult
{
    let en = TextCollator.language("en").context("English")?;
    assert_eq!(
        TextCollator.compare_bytes(&en, b"\xff", "\u{fffd}".as_bytes()),
        Ordering::Equal
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
