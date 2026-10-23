/*
 * Copyright (c) 2015 Couchbase, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from bleve v2.5.7, analysis/lang/pt/light_stemmer_pt.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_common_utils::go_utf8;

/// bleve's `stemmer_pt_light` on one term.
pub(crate) fn stem(term: &[u8]) -> Vec<u8> {
    let mut s = go_utf8::runes(term);
    if s.len() < 4 {
        return go_utf8::encode(&s);
    }
    s = remove_suffix(s);
    if s.len() > 3 && s[s.len() - 1] == 'a' {
        s = norm_feminine(s);
    }
    if s.len() > 4 && matches!(s[s.len() - 1], 'e' | 'a' | 'o') {
        s.pop();
    }
    for r in s.iter_mut() {
        *r = match *r {
            'à' | 'á' | 'â' | 'ä' | 'ã' => 'a',
            'ò' | 'ó' | 'ô' | 'ö' | 'õ' => 'o',
            'è' | 'é' | 'ê' | 'ë' => 'e',
            'ù' | 'ú' | 'û' | 'ü' => 'u',
            'ì' | 'í' | 'î' | 'ï' => 'i',
            'ç' => 'c',
            c => c,
        };
    }
    go_utf8::encode(&s)
}

fn ends_with(s: &[char], suffix: &str) -> bool {
    let suffix: Vec<char> = suffix.chars().collect();
    s.ends_with(&suffix)
}

fn remove_suffix(mut s: Vec<char>) -> Vec<char> {
    let l = s.len();
    if l > 4 && ends_with(&s, "es") && matches!(s[l - 3], 'r' | 's' | 'l' | 'z') {
        s.truncate(l - 2);
        return s;
    }
    if l > 3 && ends_with(&s, "ns") {
        s[l - 2] = 'm';
        s.truncate(l - 1);
        return s;
    }
    if l > 4 && (ends_with(&s, "eis") || ends_with(&s, "éis")) {
        s[l - 3] = 'e';
        s[l - 2] = 'l';
        s.truncate(l - 1);
        return s;
    }
    if l > 4 && ends_with(&s, "ais") {
        s[l - 2] = 'l';
        s.truncate(l - 1);
        return s;
    }
    if l > 4 && ends_with(&s, "óis") {
        s[l - 3] = 'o';
        s[l - 2] = 'l';
        s.truncate(l - 1);
        return s;
    }
    if l > 4 && ends_with(&s, "is") {
        s[l - 1] = 'l';
        return s;
    }
    if l > 3 && (ends_with(&s, "ões") || ends_with(&s, "ães")) {
        s.truncate(l - 1);
        s[l - 3] = 'ã';
        s[l - 2] = 'o';
        return s;
    }
    if l > 6 && ends_with(&s, "mente") {
        s.truncate(l - 5);
        return s;
    }
    if l > 3 && s[l - 1] == 's' {
        s.truncate(l - 1);
    }
    s
}

fn norm_feminine(mut s: Vec<char>) -> Vec<char> {
    let l = s.len();
    if l > 7 && (ends_with(&s, "inha") || ends_with(&s, "iaca") || ends_with(&s, "eira")) {
        s[l - 1] = 'o';
        return s;
    }
    if l > 6 {
        if ["osa", "ica", "ida", "ada", "iva", "ama"]
            .iter()
            .any(|x| ends_with(&s, x))
        {
            s[l - 1] = 'o';
            return s;
        }
        if ends_with(&s, "ona") {
            s[l - 3] = 'ã';
            s[l - 2] = 'o';
            s.truncate(l - 1);
            return s;
        }
        if ends_with(&s, "ora") {
            s.truncate(l - 1);
            return s;
        }
        if ends_with(&s, "esa") {
            s[l - 3] = 'ê';
            s.truncate(l - 1);
            return s;
        }
        if ends_with(&s, "na") {
            s[l - 1] = 'o';
            return s;
        }
    }
    s
}
