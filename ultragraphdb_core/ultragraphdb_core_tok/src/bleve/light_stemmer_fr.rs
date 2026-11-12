/*
 * Copyright (c) 2015 Couchbase, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from bleve v2.5.7, analysis/lang/fr/light_stemmer_fr.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_common_utils::go_unicode;

/// bleve's `stemmer_fr_light` on one term.
pub(crate) fn stem(term: &str) -> String {
    stem_runes(term.chars().collect()).iter().collect()
}

fn ends_with(s: &[char], suffix: &str) -> bool {
    let suffix: Vec<char> = suffix.chars().collect();
    s.ends_with(&suffix)
}

/// Sets the letter `from_end` places from the end, 1 the last; every caller has checked that the word is that long.
fn set(s: &mut [char], from_end: usize, c: char) {
    if let Some(slot) = s.len().checked_sub(from_end).and_then(|i| s.get_mut(i)) {
        *slot = c;
    }
}

fn cut(mut s: Vec<char>, n: usize) -> Vec<char> {
    s.truncate(s.len() - n);
    s
}

/// Where a stage of the stemmer leaves a word: stemmed, to be normalised, or passed on with the length the next
/// stage reads, which a stage updates only where bleve does.
enum Step {
    Stemmed(Vec<char>),
    Next(Vec<char>, usize),
}

fn stem_runes(s: Vec<char>) -> Vec<char> {
    let (s, l) = plural(s);
    let (s, l) = match agents(s, l) {
        Step::Stemmed(s) => return norm(s),
        Step::Next(s, l) => (s, l),
    };
    let (s, l) = match feminines(s, l) {
        Step::Stemmed(s) => return norm(s),
        Step::Next(s, l) => (s, l),
    };
    match adjectives(s, l) {
        Step::Stemmed(s) => norm(s),
        Step::Next(s, l) => nouns(s, l),
    }
}

/// The plural and `-aux` endings removed, and the length the stages read.
fn plural(mut s: Vec<char>) -> (Vec<char>, usize) {
    let mut l = s.len();
    if l > 5 && s.last() == Some(&'x') {
        if let [.., before, 'a', u @ 'u', 'x'] = s.as_mut_slice()
            && *before != 'e'
        {
            *u = 'l';
        }
        s.truncate(l - 1);
        l = s.len();
    }
    if l > 3 && s.last() == Some(&'x') {
        s.truncate(l - 1);
        l = s.len();
    }
    if l > 3 && s.last() == Some(&'s') {
        s.truncate(l - 1);
        l = s.len();
    }
    (s, l)
}

/// The `-issement`, `-issant`, `-ement`, `-ficatrice`, `-ficateur`, `-catrice` and `-cateur` endings.
fn agents(mut s: Vec<char>, l: usize) -> Step {
    if l > 9 && ends_with(&s, "issement") {
        s = cut(s, 6);
        set(&mut s, 1, 'r');
        return Step::Stemmed(s);
    }
    if l > 8 && ends_with(&s, "issant") {
        s = cut(s, 4);
        set(&mut s, 1, 'r');
        return Step::Stemmed(s);
    }
    if l > 6 && ends_with(&s, "ement") {
        s = cut(s, 4);
        if s.len() > 3 && ends_with(&s, "ive") {
            s = cut(s, 1);
            set(&mut s, 1, 'f');
        }
        return Step::Stemmed(s);
    }
    if l > 11 && ends_with(&s, "ficatrice") {
        s = cut(s, 5);
        set(&mut s, 2, 'e');
        set(&mut s, 1, 'r');
        return Step::Stemmed(s);
    }
    if l > 10 && ends_with(&s, "ficateur") {
        s = cut(s, 4);
        set(&mut s, 2, 'e');
        set(&mut s, 1, 'r');
        return Step::Stemmed(s);
    }
    if l > 9 && ends_with(&s, "catrice") {
        s = cut(s, 3);
        set(&mut s, 4, 'q');
        set(&mut s, 3, 'u');
        set(&mut s, 2, 'e');
        return Step::Stemmed(s);
    }
    if l > 8 && ends_with(&s, "cateur") {
        s = cut(s, 2);
        set(&mut s, 4, 'q');
        set(&mut s, 3, 'u');
        set(&mut s, 2, 'e');
        set(&mut s, 1, 'r');
        return Step::Stemmed(s);
    }
    Step::Next(s, l)
}

/// The `-atrice`, `-ateur`, `-trice`, `-ième`, `-teuse`, `-teur` and `-euse` endings.
fn feminines(mut s: Vec<char>, mut l: usize) -> Step {
    if l > 8 && ends_with(&s, "atrice") {
        s = cut(s, 4);
        set(&mut s, 2, 'e');
        set(&mut s, 1, 'r');
        return Step::Stemmed(s);
    }
    if l > 7 && ends_with(&s, "ateur") {
        s = cut(s, 3);
        set(&mut s, 2, 'e');
        set(&mut s, 1, 'r');
        return Step::Stemmed(s);
    }
    if l > 6 && ends_with(&s, "trice") {
        s = cut(s, 1);
        l = s.len();
        set(&mut s, 3, 'e');
        set(&mut s, 2, 'u');
        set(&mut s, 1, 'r');
    }
    if l > 5 && ends_with(&s, "ième") {
        return Step::Stemmed(cut(s, 4));
    }
    if l > 7 && ends_with(&s, "teuse") {
        s = cut(s, 2);
        set(&mut s, 1, 'r');
        return Step::Stemmed(s);
    }
    if l > 6 && ends_with(&s, "teur") {
        s = cut(s, 1);
        set(&mut s, 1, 'r');
        return Step::Stemmed(s);
    }
    if l > 5 && ends_with(&s, "euse") {
        return Step::Stemmed(cut(s, 2));
    }
    Step::Next(s, l)
}

/// The `-ère`, `-ive`, `-folle`, `-molle`, `-nnelle`, `-nnel`, `-ète` and `-ique` endings.
fn adjectives(mut s: Vec<char>, mut l: usize) -> Step {
    if l > 8 && ends_with(&s, "ère") {
        s = cut(s, 1);
        set(&mut s, 2, 'e');
        return Step::Stemmed(s);
    }
    if l > 7 && ends_with(&s, "ive") {
        s = cut(s, 1);
        set(&mut s, 1, 'f');
        return Step::Stemmed(s);
    }
    if l > 4 && (ends_with(&s, "folle") || ends_with(&s, "molle")) {
        s = cut(s, 2);
        set(&mut s, 1, 'u');
        return Step::Stemmed(s);
    }
    if l > 9 && ends_with(&s, "nnelle") {
        return Step::Stemmed(cut(s, 5));
    }
    if l > 9 && ends_with(&s, "nnel") {
        return Step::Stemmed(cut(s, 3));
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
    Step::Next(s, l)
}

/// The `-esse`, `-inage`, `-isation`, `-isateur`, `-ation` and `-ition` endings, and the word normalised.
fn nouns(mut s: Vec<char>, l: usize) -> Vec<char> {
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
        while let Some(r) = s.get_mut(i) {
            *r = match *r {
                'à' | 'á' | 'â' => 'a',
                'ô' => 'o',
                'è' | 'é' | 'ê' => 'e',
                'ù' | 'û' => 'u',
                'î' => 'i',
                'ç' => 'c',
                c => c,
            };
            remove_doubled_letters(&mut s);
            i += 1;
        }
    }
    if s.len() > 4 && ends_with(&s, "ie") {
        let n = s.len();
        s.truncate(n - 2);
    }
    if s.len() > 4 {
        if s.last() == Some(&'r') {
            let _last: Option<char> = s.pop();
        }
        if s.last() == Some(&'e') {
            let _last: Option<char> = s.pop();
        }
        if s.last() == Some(&'e') {
            let _last: Option<char> = s.pop();
        }
        if matches!(s.as_slice(), [.., a, b] if a == b && go_unicode::is_letter(*b)) {
            let _last: Option<char> = s.pop();
        }
    }
    s
}

/// bleve's doubled-letter removal: a letter that repeats the one kept before it is removed.
fn remove_doubled_letters(s: &mut Vec<char>) {
    let Some(&first) = s.first() else {
        return;
    };
    let mut ch = first;
    let mut j = 1;
    while let Some(&c) = s.get(j) {
        if c == ch && go_unicode::is_letter(ch) {
            let _duplicate: char = s.remove(j);
        } else {
            ch = c;
            j += 1;
        }
    }
}
