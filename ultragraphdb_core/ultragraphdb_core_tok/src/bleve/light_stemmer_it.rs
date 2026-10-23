/*
 * Copyright (c) 2015 Couchbase, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from bleve v2.5.7, analysis/lang/it/light_stemmer_it.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_common_utils::go_utf8;

/// bleve's `stemmer_it_light` on one term.
pub(crate) fn stem(term: &[u8]) -> Vec<u8> {
    let mut s = go_utf8::runes(term);
    let l = s.len();
    if l < 6 {
        return go_utf8::encode(&s);
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
    let prev = s[l - 2];
    let keep = match s[l - 1] {
        'e' | 'i' if prev == 'i' || prev == 'h' => l - 2,
        'a' | 'o' if prev == 'i' => l - 2,
        'e' | 'i' | 'a' | 'o' => l - 1,
        _ => l,
    };
    s.truncate(keep);
    go_utf8::encode(&s)
}
