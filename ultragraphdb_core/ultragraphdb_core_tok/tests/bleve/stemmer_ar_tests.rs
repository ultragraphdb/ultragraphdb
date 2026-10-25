/*
 * Copyright (c) 2014 Couchbase, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from bleve v2.5.7, analysis/lang/ar/stemmer_ar_test.go. Changed for ultragraphdb: translated to Rust;
 * every case of the Go table, through the filter Dgraph names, beside the edge inputs with Go's stems.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::bleve::check_stems;

#[test]
fn test_arabic_stemmer_filter() {
    check_stems(
        "stemmer_ar",
        &[
            ("الحسن", "حسن"),
            ("والحسن", "حسن"),
            ("بالحسن", "حسن"),
            ("كالحسن", "حسن"),
            ("فالحسن", "حسن"),
            ("للاخر", "اخر"),
            ("وحسن", "حسن"),
            ("زوجها", "زوج"),
            ("ساهدان", "ساهد"),
            ("ساهدات", "ساهد"),
            ("ساهدون", "ساهد"),
            ("ساهدين", "ساهد"),
            ("ساهديه", "ساهد"),
            ("ساهدية", "ساهد"),
            ("ساهده", "ساهد"),
            ("ساهدة", "ساهد"),
            ("ساهدي", "ساهد"),
            ("وساهدون", "ساهد"),
            ("ساهدهات", "ساهد"),
            ("الو", "الو"),
            ("English", "English"),
            ("سلام", "سلام"),
            ("السلام", "سلام"),
            ("سلامة", "سلام"),
            ("السلامة", "سلام"),
            ("الوصل", "وصل"),
            ("والصل", "صل"),
            ("", ""),
        ],
        &[],
    );
}
