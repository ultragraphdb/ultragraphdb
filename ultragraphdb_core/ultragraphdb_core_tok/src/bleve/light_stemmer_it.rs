/*
 * Copyright (c) 2015 Couchbase, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from bleve v2.5.7, analysis/lang/it/light_stemmer_it.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

/// bleve's `stemmer_it_light` on one term.
pub(crate) fn stem(term: &str) -> String {
    let mut s: Vec<char> = term.chars().collect();
    let l = s.len();
    if l < 6 {
        return s.iter().collect();
    }
    for r in s.iter_mut() {
        *r = match *r {
            'à' | 'á' | 'â' | 'ä' => 'a',
            'ò' | 'ó' | 'ô' | 'ö' => 'o',
            'è' | 'é' | 'ê' | 'ë' => 'e',
            'ù' | 'ú' | 'û' | 'ü' => 'u',
            'ì' | 'í' | 'î' | 'ï' => 'i',
            c => c,
        };
    }
    let keep = match s.as_slice() {
        [.., 'i' | 'h', 'e' | 'i'] => l - 2,
        [.., 'i', 'a' | 'o'] => l - 2,
        [.., 'e' | 'i' | 'a' | 'o'] => l - 1,
        _ => l,
    };
    s.truncate(keep);
    s.iter().collect()
}
