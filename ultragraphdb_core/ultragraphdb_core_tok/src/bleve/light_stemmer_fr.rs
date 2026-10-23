/*
 * Copyright (c) 2015 Couchbase, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from bleve v2.5.7, analysis/lang/fr/light_stemmer_fr.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_common_utils::{go_unicode, go_utf8};

/// bleve's `stemmer_fr_light` on one term.
pub(crate) fn stem(term: &[u8]) -> Vec<u8> {
    go_utf8::encode(&stem_runes(go_utf8::runes(term)))
}

fn ends_with(s: &[char], suffix: &str) -> bool {
    let suffix: Vec<char> = suffix.chars().collect();
    s.ends_with(&suffix)
}

fn set(s: &mut [char], from_end: usize, c: char) {
    let n = s.len();
    s[n - from_end] = c;
}

fn cut(mut s: Vec<char>, n: usize) -> Vec<char> {
    s.truncate(s.len() - n);
    s
}

fn stem_runes(mut s: Vec<char>) -> Vec<char> {
    let mut l = s.len();
    if l > 5 && s[l - 1] == 'x' {
        if s[l - 3] == 'a' && s[l - 2] == 'u' && s[l - 4] != 'e' {
            s[l - 2] = 'l';
        }
        s.truncate(l - 1);
        l = s.len();
    }
    if l > 3 && s[l - 1] == 'x' {
        s.truncate(l - 1);
        l = s.len();
    }
    if l > 3 && s[l - 1] == 's' {
        s.truncate(l - 1);
        l = s.len();
    }
    if l > 9 && ends_with(&s, "issement") {
        s = cut(s, 6);
        set(&mut s, 1, 'r');
        return norm(s);
    }
    if l > 8 && ends_with(&s, "issant") {
        s = cut(s, 4);
        set(&mut s, 1, 'r');
        return norm(s);
    }
    if l > 6 && ends_with(&s, "ement") {
        s = cut(s, 4);
        if s.len() > 3 && ends_with(&s, "ive") {
            s = cut(s, 1);
            set(&mut s, 1, 'f');
        }
        return norm(s);
    }
    if l > 11 && ends_with(&s, "ficatrice") {
        s = cut(s, 5);
        set(&mut s, 2, 'e');
        set(&mut s, 1, 'r');
        return norm(s);
    }
    if l > 10 && ends_with(&s, "ficateur") {
        s = cut(s, 4);
        set(&mut s, 2, 'e');
        set(&mut s, 1, 'r');
        return norm(s);
    }
    if l > 9 && ends_with(&s, "catrice") {
        s = cut(s, 3);
        set(&mut s, 4, 'q');
        set(&mut s, 3, 'u');
        set(&mut s, 2, 'e');
        return norm(s);
    }
    if l > 8 && ends_with(&s, "cateur") {
        s = cut(s, 2);
        set(&mut s, 4, 'q');
        set(&mut s, 3, 'u');
        set(&mut s, 2, 'e');
        set(&mut s, 1, 'r');
        return norm(s);
    }
    if l > 8 && ends_with(&s, "atrice") {
        s = cut(s, 4);
        set(&mut s, 2, 'e');
        set(&mut s, 1, 'r');
        return norm(s);
    }
    if l > 7 && ends_with(&s, "ateur") {
        s = cut(s, 3);
        set(&mut s, 2, 'e');
        set(&mut s, 1, 'r');
        return norm(s);
    }
    if l > 6 && ends_with(&s, "trice") {
        s = cut(s, 1);
        l = s.len();
        set(&mut s, 3, 'e');
        set(&mut s, 2, 'u');
        set(&mut s, 1, 'r');
    }
    if l > 5 && ends_with(&s, "ième") {
        return norm(cut(s, 4));
    }
    if l > 7 && ends_with(&s, "teuse") {
        s = cut(s, 2);
        set(&mut s, 1, 'r');
        return norm(s);
    }
    if l > 6 && ends_with(&s, "teur") {
        s = cut(s, 1);
        set(&mut s, 1, 'r');
        return norm(s);
    }
    if l > 5 && ends_with(&s, "euse") {
        return norm(cut(s, 2));
    }
    if l > 8 && ends_with(&s, "ère") {
        s = cut(s, 1);
        set(&mut s, 2, 'e');
        return norm(s);
    }
    if l > 7 && ends_with(&s, "ive") {
        s = cut(s, 1);
        set(&mut s, 1, 'f');
        return norm(s);
    }
    if l > 4 && (ends_with(&s, "folle") || ends_with(&s, "molle")) {
        s = cut(s, 2);
        set(&mut s, 1, 'u');
        return norm(s);
    }
    if l > 9 && ends_with(&s, "nnelle") {
        return norm(cut(s, 5));
    }
    if l > 9 && ends_with(&s, "nnel") {
        return norm(cut(s, 3));
    }
    if l > 4 && ends_with(&s, "ète") {
        s = cut(s, 1);
        l = s.len();
        set(&mut s, 2, 'e');
    }
    if l > 8 && ends_with(&s, "ique") {
        s = cut(s, 4);
        l = s.len();
    }
    if l > 8 && ends_with(&s, "esse") {
        return norm(cut(s, 3));
    }
    if l > 7 && ends_with(&s, "inage") {
        return norm(cut(s, 3));
    }
    if l > 9 && ends_with(&s, "isation") {
        s = cut(s, 7);
        if s.len() > 5 && ends_with(&s, "ual") {
            set(&mut s, 2, 'e');
        }
        return norm(s);
    }
    if l > 9 && ends_with(&s, "isateur") {
        return norm(cut(s, 7));
    }
    if l > 8 && ends_with(&s, "ation") {
        return norm(cut(s, 5));
    }
    if l > 8 && ends_with(&s, "ition") {
        return norm(cut(s, 5));
    }
    norm(s)
}

/// bleve's `norm`. Its accent loop runs the doubled-letter removal once per rune, inside the loop, as bleve does.
fn norm(mut s: Vec<char>) -> Vec<char> {
    if s.len() > 4 {
        let mut i = 0;
        while i < s.len() {
            s[i] = match s[i] {
                'à' | 'á' | 'â' => 'a',
                'ô' => 'o',
                'è' | 'é' | 'ê' => 'e',
                'ù' | 'û' => 'u',
                'î' => 'i',
                'ç' => 'c',
                c => c,
            };
            let mut ch = s[0];
            let mut j = 1;
            while j < s.len() {
                if s[j] == ch && go_unicode::is_letter(ch) {
                    s.remove(j);
                } else {
                    ch = s[j];
                    j += 1;
                }
            }
            i += 1;
        }
    }
    if s.len() > 4 && ends_with(&s, "ie") {
        let n = s.len();
        s.truncate(n - 2);
    }
    if s.len() > 4 {
        if s[s.len() - 1] == 'r' {
            s.pop();
        }
        if s[s.len() - 1] == 'e' {
            s.pop();
        }
        if s[s.len() - 1] == 'e' {
            s.pop();
        }
        let n = s.len();
        if s[n - 1] == s[n - 2] && go_unicode::is_letter(s[n - 1]) {
            s.pop();
        }
    }
    s
}
