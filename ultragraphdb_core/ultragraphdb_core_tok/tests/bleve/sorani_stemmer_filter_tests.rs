/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! bleve v2.5.7's analysis/lang/ckb/sorani_stemmer_filter_test.go runs the Sorani normalizer before the stemmer, which
//! Dgraph does not; the expected stems are the stemmer's alone on that test's inputs, by a Go program over bleve's
//! filter. Unnormalized, `ك` (Arabic kaf) is not `ک`, so `-كە` loses only its `ە`.

use crate::bleve::check_stems;

#[test]
fn test_sorani_stemmer_filter() {
    check_stems(
        "stemmer_ckb",
        &[
            ("پیاوێک", "پیاو"),
            ("دەرگایەک", "دەرگا"),
            ("پیاوەكە", "پیاوەك"),
            ("دەرگاكە", "دەرگاك"),
            ("کتاویە", "کتاوی"),
            ("دەرگایە", "دەرگا"),
            ("پیاوان", "پیاو"),
            ("دەرگایان", "دەرگا"),
            ("پیاوەکان", "پیاو"),
            ("دەرگاکان", "دەرگا"),
            ("پیاوانە", "پیاو"),
            ("دەرگایانە", "دەرگا"),
            ("هۆتیلی", "هۆتیل"),
            ("هۆتیلێکی", "هۆتیل"),
            ("هۆتیلانی", "هۆتیل"),
            ("دوورەوە", "دوور"),
            ("نیوەشەودا", "نیوەشەو"),
            ("سۆرانا", "سۆران"),
            ("پارەمان", "پارە"),
            ("پارەتان", "پارە"),
            ("پارەیان", "پارە"),
        ],
        &[],
    );
}
