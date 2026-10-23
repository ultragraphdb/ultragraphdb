/*
 * Copyright (c) 2017 Couchbase, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from bleve v2.5.7, analysis/lang/es/light_stemmer_es.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

/// bleve's `stemmer_es_light` on one term.
pub(crate) fn stem(term: &str) -> String {
    let mut s: Vec<char> = term.chars().collect();
    let l = s.len();
    if l >= 5 {
        match s[l - 1] {
            'o' | 'a' | 'e' => s.truncate(l - 1),
            's' => {
                if s[l - 2] == 'e' && s[l - 3] == 's' && s[l - 4] == 'e' {
                    s.truncate(l - 2);
                } else if s[l - 2] == 'e' && s[l - 3] == 'c' {
                    s[l - 3] = 'z';
                    s.truncate(l - 2);
                } else if matches!(s[l - 2], 'o' | 'a' | 'e') {
                    s.truncate(l - 2);
                }
            }
            _ => {}
        }
    }
    s.iter().collect()
}
