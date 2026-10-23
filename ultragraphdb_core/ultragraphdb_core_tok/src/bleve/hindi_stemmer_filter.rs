/*
 * Copyright (c) 2014 Couchbase, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from bleve v2.5.7, analysis/lang/hi/hindi_stemmer_filter.go. Changed for ultragraphdb: translated to
 * Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_common_utils::go_utf8;

/// The suffixes bleve removes, longest first: the rune count a term must exceed, the runes removed and the
/// suffixes.
const GROUPS: [(usize, usize, &[&str]); 5] = [
    (
        6,
        5,
        &["ाएंगी", "ाएंगे", "ाऊंगी", "ाऊंगा", "ाइयाँ", "ाइयों", "ाइयां"],
    ),
    (
        5,
        4,
        &[
            "ाएगी",
            "ाएगा",
            "ाओगी",
            "ाओगे",
            "एंगी",
            "ेंगी",
            "एंगे",
            "ेंगे",
            "ूंगी",
            "ूंगा",
            "ातीं",
            "नाओं",
            "नाएं",
            "ताओं",
            "ताएं",
            "ियाँ",
            "ियों",
            "ियां",
        ],
    ),
    (
        4,
        3,
        &[
            "ाकर",
            "ाइए",
            "ाईं",
            "ाया",
            "ेगी",
            "ेगा",
            "ोगी",
            "ोगे",
            "ाने",
            "ाना",
            "ाते",
            "ाती",
            "ाता",
            "तीं",
            "ाओं",
            "ाएं",
            "ुओं",
            "ुएं",
            "ुआं",
        ],
    ),
    (
        3,
        2,
        &[
            "कर", "ाओ", "िए", "ाई", "ाए", "ने", "नी", "ना", "ते", "ीं", "ती", "ता", "ाँ", "ां", "ों", "ें",
        ],
    ),
    (2, 1, &["ो", "े", "ू", "ु", "ी", "ि", "ा"]),
];

/// bleve's `stemmer_hi` on one term.
pub(crate) fn stem(term: &[u8]) -> Vec<u8> {
    let r = go_utf8::runes(term);
    for (min, n, suffixes) in GROUPS {
        if r.len() > min && suffixes.iter().any(|s| term.ends_with(s.as_bytes())) {
            return go_utf8::encode(&r[..r.len() - n]);
        }
    }
    term.to_vec()
}
