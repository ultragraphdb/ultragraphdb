/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::cmp::Ordering;

use ultragraphdb_traits_collator::Collator;

/// A collator that knows one language, `rev`, whose order is the reverse of the bytes' order.
struct Reverse;

impl Collator for Reverse {
    type Language = &'static str;

    fn language(&self, lang: &str) -> Option<Self::Language> {
        (lang == "rev").then_some("rev")
    }

    fn compare_bytes(&self, _: &Self::Language, a: &[u8], b: &[u8]) -> Ordering {
        b.cmp(a)
    }
}

#[test]
fn test_language_refuses_what_the_collator_does_not_know() {
    assert_eq!(Reverse.language("rev"), Some("rev"));
    assert_eq!(Reverse.language("en"), None);
}

#[test]
fn test_compare_is_compare_bytes_over_the_strings_bytes() {
    let rev = Reverse.language("rev").unwrap();
    assert_eq!(Reverse.compare(&rev, "a", "b"), Ordering::Greater);
    assert_eq!(Reverse.compare(&rev, "b", "a"), Ordering::Less);
    assert_eq!(Reverse.compare(&rev, "é", "é"), Ordering::Equal);
    assert_eq!(
        Reverse.compare(&rev, "a\u{301}", "\u{e1}"),
        Reverse.compare_bytes(&rev, "a\u{301}".as_bytes(), "\u{e1}".as_bytes())
    );
}
