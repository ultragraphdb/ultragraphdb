/*
 * Copyright 2012 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from golang.org/x/text v0.41.0, collate/collate_test.go (TestNumeric) and collate/option_test.go
 * (TestAlternateSortTypes). Changed for ultragraphdb: translated to Rust; the options Loose and Numeric are set
 * through the tag (ks-level1, kn-true), the only way Dgraph sets options.
 * Copyright (c) 2026. Marvin Hansen.
 */

use std::cmp::Ordering;

use ultragraphdb_core_text::{Collator, Tag};

#[test]
fn numeric() {
    let c = Collator::new(&Tag::parse("en-u-kn-true-ks-level1").unwrap());
    let tests = [
        ("1", "2", Ordering::Less),
        ("2", "12", Ordering::Less),
        ("２", "１２", Ordering::Less), // Fullwidth is sorted as usual.
        ("₂", "₁₂", Ordering::Greater), // Subscript is not sorted as numbers.
        ("②", "①②", Ordering::Greater), // Circled is not sorted as numbers.
        // Imperial Aramaic, is not sorted as number.
        ("\u{10859}", "\u{10858}\u{10859}", Ordering::Greater),
        ("12", "2", Ordering::Greater),
        ("A-1", "A-2", Ordering::Less),
        ("A-2", "A-12", Ordering::Less),
        ("A-12", "A-2", Ordering::Greater),
        ("A-0001", "A-1", Ordering::Equal),
    ];
    for (i, (a, b, want)) in tests.into_iter().enumerate() {
        assert_eq!(c.compare(a, b), want, "{i}: compare({a}, {b})");
    }
}

#[test]
fn alternate_sort_types() {
    let cases = [
        (
            "zh,cmn,zh-Hant-u-co-pinyin,zh-HK-u-co-pinyin,zh-pinyin",
            ["爸爸", "妈妈", "儿子", "女儿"],
            ["爸爸", "儿子", "妈妈", "女儿"],
        ),
        (
            "zh-Hant,zh-u-co-stroke,zh-Hant-u-co-stroke",
            ["爸爸", "妈妈", "儿子", "女儿"],
            ["儿子", "女儿", "妈妈", "爸爸"],
        ),
    ];
    for (langs, input, want) in cases {
        for lang in langs.split(',') {
            let c = Collator::new(&Tag::parse(lang).unwrap());
            let mut got = input;
            got.sort_by_key(|s| c.key(s));
            assert_eq!(got, want, "{lang}");
        }
    }
}
