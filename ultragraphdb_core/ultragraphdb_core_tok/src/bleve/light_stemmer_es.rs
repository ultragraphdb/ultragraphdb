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
        let keep = match s.as_mut_slice() {
            [.., 'o' | 'a' | 'e'] => l - 1,
            [.., 'e', 's', 'e', 's'] => l - 2,
            [.., c @ 'c', 'e', 's'] => {
                *c = 'z';
                l - 2
            }
            [.., 'o' | 'a' | 'e', 's'] => l - 2,
            _ => l,
        };
        s.truncate(keep);
    }
    s.iter().collect()
}
