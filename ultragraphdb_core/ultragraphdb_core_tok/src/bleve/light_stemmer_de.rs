/*
 * Copyright (c) 2017 Couchbase, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from bleve v2.5.7, analysis/lang/de/light_stemmer_de.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

/// bleve's `stemmer_de_light` on one term.
pub(crate) fn stem(term: &str) -> String {
    let mut s: Vec<char> = term.chars().collect();
    for r in s.iter_mut() {
        *r = match *r {
            'ä' | 'à' | 'á' | 'â' => 'a',
            'ö' | 'ò' | 'ó' | 'ô' => 'o',
            'ï' | 'ì' | 'í' | 'î' => 'i',
            'ü' | 'ù' | 'ú' | 'û' => 'u',
            c => c,
        };
    }
    s.truncate(step1(&s));
    s.truncate(step2(&s));
    s.iter().collect()
}

fn st_ending(c: char) -> bool {
    matches!(c, 'b' | 'd' | 'f' | 'g' | 'h' | 'k' | 'l' | 'm' | 'n' | 't')
}

/// The length step 1 keeps.
fn step1(s: &[char]) -> usize {
    let l = s.len();
    match s {
        [.., 'e', 'r', 'n'] if l > 5 => l - 3,
        [.., 'e', 'm' | 'n' | 'r' | 's'] if l > 4 => l - 2,
        [.., 'e'] if l > 3 => l - 1,
        [.., before, 's'] if l > 3 && st_ending(*before) => l - 1,
        _ => l,
    }
}

/// The length step 2 keeps.
fn step2(s: &[char]) -> usize {
    let l = s.len();
    match s {
        [.., 'e', 's', 't'] if l > 5 => l - 3,
        [.., 'e', 'r' | 'n'] if l > 4 => l - 2,
        [.., before, 's', 't'] if l > 4 && st_ending(*before) => l - 2,
        _ => l,
    }
}
