/*
 * Copyright (c) 2015 Couchbase, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from bleve v2.5.7, analysis/lang/pt/light_stemmer_pt.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

/// bleve's `stemmer_pt_light` on one term.
pub(crate) fn stem(term: &str) -> String {
    let mut s: Vec<char> = term.chars().collect();
    if s.len() < 4 {
        return s.iter().collect();
    }
    s = remove_suffix(s);
    if s.len() > 3 && s.last() == Some(&'a') {
        s = norm_feminine(s);
    }
    if s.len() > 4 && matches!(s.last(), Some('e' | 'a' | 'o')) {
        let _last: Option<char> = s.pop();
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
    s.iter().collect()
}

fn ends_with(s: &[char], suffix: &str) -> bool {
    let suffix: Vec<char> = suffix.chars().collect();
    s.ends_with(&suffix)
}

/// The plural and adverb endings removed: the first ending matched, where the word is long enough, decides.
fn remove_suffix(mut s: Vec<char>) -> Vec<char> {
    let l = s.len();
    let keep = match s.as_mut_slice() {
        [.., 'r' | 's' | 'l' | 'z', 'e', 's'] if l > 4 => l - 2,
        [.., n @ 'n', 's'] if l > 3 => {
            *n = 'm';
            l - 1
        }
        [.., e @ ('e' | 'é'), i @ 'i', 's'] if l > 4 => {
            *e = 'e';
            *i = 'l';
            l - 1
        }
        [.., 'a', i @ 'i', 's'] if l > 4 => {
            *i = 'l';
            l - 1
        }
        [.., o @ 'ó', i @ 'i', 's'] if l > 4 => {
            *o = 'o';
            *i = 'l';
            l - 1
        }
        [.., 'i', last @ 's'] if l > 4 => {
            *last = 'l';
            l
        }
        [.., a @ ('õ' | 'ã'), e @ 'e', 's'] if l > 3 => {
            *a = 'ã';
            *e = 'o';
            l - 1
        }
        [.., 'm', 'e', 'n', 't', 'e'] if l > 6 => l - 5,
        [.., 's'] if l > 3 => l - 1,
        _ => l,
    };
    s.truncate(keep);
    s
}

fn norm_feminine(mut s: Vec<char>) -> Vec<char> {
    let l = s.len();
    if l > 7 && (ends_with(&s, "inha") || ends_with(&s, "iaca") || ends_with(&s, "eira")) {
        if let Some(last) = s.last_mut() {
            *last = 'o';
        }
        return s;
    }
    if l > 6 {
        if ["osa", "ica", "ida", "ada", "iva", "ama"]
            .iter()
            .any(|x| ends_with(&s, x))
        {
            if let Some(last) = s.last_mut() {
                *last = 'o';
            }
            return s;
        }
        let keep = match s.as_mut_slice() {
            [.., o @ 'o', n @ 'n', 'a'] => {
                *o = 'ã';
                *n = 'o';
                l - 1
            }
            [.., 'o', 'r', 'a'] => l - 1,
            [.., e @ 'e', 's', 'a'] => {
                *e = 'ê';
                l - 1
            }
            [.., 'n', last @ 'a'] => {
                *last = 'o';
                l
            }
            _ => l,
        };
        s.truncate(keep);
    }
    s
}
