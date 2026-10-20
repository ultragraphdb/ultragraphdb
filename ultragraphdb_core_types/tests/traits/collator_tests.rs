/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::cmp::Ordering;

use ultragraphdb_core_types::{Collator, TextCollator};

#[test]
fn test_text_collator_refuses_what_go_refuses() {
    assert!(TextCollator.language("xx-invalid-tag-!").is_none());
    assert!(TextCollator.language("xx").is_none());
    assert!(TextCollator.language("en_US").is_some());
    assert!(TextCollator.language("und").is_some());
}

#[test]
fn test_text_collator_compares_as_compare_string() {
    let de = TextCollator.language("de").unwrap();
    assert_eq!(
        TextCollator.compare(&de, "öffnen", "zumachen"),
        Ordering::Less
    );
    let sv = TextCollator.language("sv").unwrap();
    assert_eq!(TextCollator.compare(&sv, "öppna", "zon"), Ordering::Greater);
    // CompareString, not a comparison of keys: with -u-ka-shifted Go skips the primary level.
    let shifted = TextCollator.language("en-u-ka-shifted").unwrap();
    assert_eq!(TextCollator.compare(&shifted, "a", "b"), Ordering::Equal);
    // Strings that collate equal compare by their bytes under -u-ks-identic.
    let identic = TextCollator.language("en-u-ks-identic").unwrap();
    assert_eq!(
        TextCollator.compare(&identic, "a\u{301}", "\u{e1}"),
        "a\u{301}".cmp("\u{e1}")
    );
}

#[test]
fn test_text_collator_compares_bytes_as_go_compares_strings() {
    let en = TextCollator.language("en").unwrap();
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
}
