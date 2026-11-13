/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::cmp::Ordering;

use ultragraphdb_core_text::Collator;
use ultragraphdb_tool_bdd::{Context, TestResult};

fn collator(lang: &str) -> TestResult<Collator> {
    Collator::for_language(lang).context("a collator")
}

/// A text that is no language tag has no collation, and its caller compares bytes.
#[test]
fn test_no_tag_has_no_collation() {
    for lang in ["", "x!", "en--US"] {
        assert!(Collator::for_language(lang).is_none(), "{lang:?}");
    }
}

/// The root order places letters by their alphabet, an accent after its base letter and upper case after lower:
/// where code points order `Z` before `a` and `é` after `z`.
#[test]
fn test_letters_collate_by_alphabet_not_by_code_point() -> TestResult {
    let en = collator("en")?;
    assert_eq!(en.compare("a", "Z"), Ordering::Less);
    assert_eq!(en.compare("é", "z"), Ordering::Less);
    assert_eq!(en.compare("e", "é"), Ordering::Less);
    assert_eq!(en.compare("a", "A"), Ordering::Less);
    assert_eq!(en.compare("same", "same"), Ordering::Equal);
    Ok(())
}

/// A language's tailoring changes the order: Swedish sorts `ä` after `z`, German with it under `a`; and German's
/// phonebook collation, named by the tag's `-u-co-` keyword, sorts `ä` as `ae`, after `ad`.
#[test]
fn test_a_languages_tailoring_orders_its_letters() -> TestResult {
    assert_eq!(collator("sv")?.compare("ä", "z"), Ordering::Greater);
    assert_eq!(collator("de")?.compare("ä", "z"), Ordering::Less);
    assert_eq!(collator("de")?.compare("ä", "ad"), Ordering::Less);
    assert_eq!(
        collator("de-u-co-phonebk")?.compare("ä", "ad"),
        Ordering::Greater
    );
    Ok(())
}

/// Sort keys compare, as bytes, as their strings collate, in each language.
#[test]
fn test_sort_keys_order_as_the_strings_collate() -> TestResult {
    let words = [
        "", "a", "A", "ä", "ad", "b", "é", "z", "Z", "zebra", "Äpfel", "日本",
    ];
    for lang in ["en", "de", "sv", "ja"] {
        let c = collator(lang)?;
        for a in words {
            for b in words {
                assert_eq!(
                    c.key_bytes(a.as_bytes()).cmp(&c.key_bytes(b.as_bytes())),
                    c.compare(a, b),
                    "{lang}: {a:?} against {b:?}"
                );
            }
        }
    }
    Ok(())
}

/// Bytes that are no UTF-8 collate with each invalid sequence as U+FFFD, in a comparison and in a key alike.
#[test]
fn test_invalid_utf8_collates_as_the_replacement_character() -> TestResult {
    let en = collator("en")?;
    let replacement = "\u{fffd}".as_bytes();
    assert_eq!(en.compare_bytes(b"\xff", replacement), Ordering::Equal);
    assert_eq!(en.key_bytes(b"\xff"), en.key_bytes(replacement));
    assert_eq!(en.compare_bytes(b"a\xff", b"a"), Ordering::Greater);
    assert_eq!(en.compare_bytes(b"abc", b"abd"), en.compare("abc", "abd"));
    Ok(())
}

/// A clone is the same collation.
#[test]
fn test_a_clone_collates_alike() -> TestResult {
    let sv = collator("sv")?;
    let other = sv.clone();
    assert_eq!(other.compare("ä", "z"), sv.compare("ä", "z"));
    assert_eq!(other.key_bytes(b"\xc3\xa4"), sv.key_bytes(b"\xc3\xa4"));
    Ok(())
}
