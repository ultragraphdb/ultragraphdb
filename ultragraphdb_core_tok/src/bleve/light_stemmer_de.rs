/*
 * Copyright (c) 2017 Couchbase, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from bleve v2.5.7, analysis/lang/de/light_stemmer_de.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::bleve::go_utf8::{encode, runes};

/// bleve's `stemmer_de_light` on one term.
pub(crate) fn stem(term: &[u8]) -> Vec<u8> {
    let mut s = runes(term);
    for r in s.iter_mut() {
        *r = match *r {
            'ä' | 'à' | 'á' | 'â' => 'a',
            'ö' | 'ò' | 'ó' | 'ô' => 'o',
            'ï' | 'ì' | 'í' | 'î' => 'i',
            'ü' | 'ù' | 'ú' | 'û' => 'u',
            c => c,
        };
    }
    let n = step1(&s);
    s.truncate(n);
    let n = step2(&s);
    s.truncate(n);
    encode(&s)
}

fn st_ending(c: char) -> bool {
    matches!(c, 'b' | 'd' | 'f' | 'g' | 'h' | 'k' | 'l' | 'm' | 'n' | 't')
}

/// The length step 1 keeps.
fn step1(s: &[char]) -> usize {
    let l = s.len();
    if l > 5 && s[l - 3] == 'e' && s[l - 2] == 'r' && s[l - 1] == 'n' {
        return l - 3;
    }
    if l > 4 && s[l - 2] == 'e' && matches!(s[l - 1], 'm' | 'n' | 'r' | 's') {
        return l - 2;
    }
    if l > 3 && s[l - 1] == 'e' {
        return l - 1;
    }
    if l > 3 && s[l - 1] == 's' && st_ending(s[l - 2]) {
        return l - 1;
    }
    l
}

/// The length step 2 keeps.
fn step2(s: &[char]) -> usize {
    let l = s.len();
    if l > 5 && s[l - 3] == 'e' && s[l - 2] == 's' && s[l - 1] == 't' {
        return l - 3;
    }
    if l > 4 && s[l - 2] == 'e' && (s[l - 1] == 'r' || s[l - 1] == 'n') {
        return l - 2;
    }
    if l > 4 && s[l - 2] == 's' && s[l - 1] == 't' && st_ending(s[l - 3]) {
        return l - 2;
    }
    l
}
