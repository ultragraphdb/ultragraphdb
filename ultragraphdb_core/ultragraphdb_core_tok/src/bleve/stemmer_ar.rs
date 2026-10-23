/*
 * Copyright (c) 2014 Couchbase, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from bleve v2.5.7, analysis/lang/ar/stemmer_ar.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_common_utils::go_utf8;

/// From Lucene's `ArabicStemmer`, in bleve's order.
const PREFIXES: [&str; 7] = ["ال", "وال", "بال", "كال", "فال", "لل", "و"];
const SUFFIXES: [&str; 10] = ["ها", "ان", "ات", "ون", "ين", "يه", "ية", "ه", "ة", "ي"];

/// bleve's `stemmer_ar` on one term: one prefix, then every suffix in order.
pub(crate) fn stem(term: &[u8]) -> Vec<u8> {
    let mut s = go_utf8::runes(term);
    for p in PREFIXES {
        let p: Vec<char> = p.chars().collect();
        if can_stem_prefix(&s, &p) {
            s.drain(..p.len());
            break;
        }
    }
    for x in SUFFIXES {
        let x: Vec<char> = x.chars().collect();
        if s.len() >= x.len() + 2 && s.ends_with(&x) {
            s.truncate(s.len() - x.len());
        }
    }
    go_utf8::encode(&s)
}

fn can_stem_prefix(s: &[char], p: &[char]) -> bool {
    // Wa- needs at least 3 characters left, every other prefix 2.
    if p.len() == 1 && s.len() < 4 {
        return false;
    }
    s.len() >= p.len() + 2 && s.starts_with(p)
}
