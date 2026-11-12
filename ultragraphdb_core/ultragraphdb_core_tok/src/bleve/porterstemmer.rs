/*
 * Copyright (c) 2013 Charles Iliya Krempeaux <charles@reptile.ca> :: http://changelog.ca/
 * SPDX-License-Identifier: MIT
 *
 * Ported from github.com/blevesearch/go-porterstemmer v1.0.3, porterstemmer.go (StemWithoutLowerCasing), as
 * bleve v2.5.7's analysis/token/porter/porter.go calls it. Changed for ultragraphdb: translated to Rust. The
 * MIT notice is in LICENSE-PORTERSTEMMER.
 * Copyright (c) 2026. Marvin Hansen.
 */

/// bleve's `stemmer_porter` on one term: go-porterstemmer's `StemWithoutLowerCasing` over its runes.
pub(crate) fn stem(term: &str) -> String {
    stem_without_lower_casing(term.chars().collect())
        .iter()
        .collect()
}

/// go-porterstemmer's `isConsonant` of the last letter of `s`, which Go names by its index `i` in the word, `s` being
/// the word up to it; every caller passes a letter. A `y` is a consonant at the start and after a vowel. Go recurses
/// once per `y` of a run, on its growable stack; this walks the run back to the letter before it and counts its
/// parity.
fn is_consonant(s: &[char]) -> bool {
    match s.last() {
        Some('a' | 'e' | 'i' | 'o' | 'u') => false,
        Some('y') => {
            let run = s.iter().rev().take_while(|&&c| c == 'y').count();
            // The run's first `y` follows a letter that is not a `y`, a consonant unless a vowel.
            let first = s
                .iter()
                .rev()
                .nth(run)
                .is_none_or(|&before| matches!(before, 'a' | 'e' | 'i' | 'o' | 'u'));
            first == (run % 2 == 1)
        }
        _ => true,
    }
}

/// [`is_consonant`] of every letter, in one pass.
fn consonants(s: &[char]) -> Vec<bool> {
    let mut out: Vec<bool> = Vec::with_capacity(s.len());
    for &c in s {
        let consonant = match c {
            'a' | 'e' | 'i' | 'o' | 'u' => false,
            'y' => out.last().is_none_or(|&before| !before),
            _ => true,
        };
        out.push(consonant);
    }
    out
}

/// go-porterstemmer's `measure`, the m of the word's form [C](VC)^m[V]: the number of vowels followed by a
/// consonant.
fn measure(s: &[char]) -> usize {
    consonants(s)
        .windows(2)
        .filter(|pair| matches!(pair, [false, true]))
        .count()
}

/// go-porterstemmer's `hasSuffix`: `s` ends with `suffix` and is longer than it.
fn has_suffix(s: &[char], suffix: &str) -> bool {
    let suffix: Vec<char> = suffix.chars().collect();
    s.len() > suffix.len() && s.ends_with(&suffix)
}

fn contains_vowel(s: &[char]) -> bool {
    consonants(s).contains(&false)
}

fn has_repeat_double_consonant_suffix(s: &[char]) -> bool {
    matches!(s, [.., a, b] if a == b) && is_consonant(s)
}

fn has_consonant_vowel_consonant_suffix(s: &[char]) -> bool {
    let n = s.len();
    n >= 3
        && s.get(..n - 2).is_some_and(is_consonant)
        && s.get(..n - 1).is_some_and(|before| !is_consonant(before))
        && is_consonant(s)
}

fn step1a(mut s: Vec<char>) -> Vec<char> {
    if has_suffix(&s, "sses") || has_suffix(&s, "ies") {
        s.truncate(s.len() - 2);
    } else if has_suffix(&s, "ss") {
    } else if has_suffix(&s, "s") {
        s.truncate(s.len() - 1);
    }
    s
}

/// The `ed` and `ing` branches of step 1b over the stem `s[..sub_len]`. Where Go keeps one letter of the suffix
/// (`s[..sub_len + 1]`), that letter is the `e` of `ed`, or the `i` of `ing` rewritten to `e` when `at_bl_iz_e`.
fn step1b_tail(s: Vec<char>, sub_len: usize, at_bl_iz_e: bool) -> Vec<char> {
    let mut s = s;
    // The stem precedes a suffix of the word, so it is in it.
    let Some(sub) = s.get(..sub_len).map(<[char]>::to_vec) else {
        return s;
    };
    // A stem holding a vowel holds a last letter.
    let Some(&c) = sub.last().filter(|_| contains_vowel(&sub)) else {
        return s;
    };
    if has_suffix(&sub, "at") || has_suffix(&sub, "bl") || has_suffix(&sub, "iz") {
        s.truncate(sub_len + 1);
        if at_bl_iz_e && let Some(kept) = s.last_mut() {
            *kept = 'e';
        }
        return s;
    }
    if c != 'l' && c != 's' && c != 'z' && has_repeat_double_consonant_suffix(&sub) {
        s.truncate(sub_len - 1);
        return s;
    }
    if measure(&sub) == 1
        && has_consonant_vowel_consonant_suffix(&sub)
        && c != 'w'
        && c != 'x'
        && c != 'y'
    {
        s.truncate(sub_len + 1);
        if let Some(kept) = s.last_mut() {
            *kept = 'e';
        }
        return s;
    }
    s.truncate(sub_len);
    s
}

fn step1b(mut s: Vec<char>) -> Vec<char> {
    let n = s.len();
    if has_suffix(&s, "eed") {
        if s.get(..n - 3).is_some_and(|stem| measure(stem) > 0) {
            s.truncate(n - 1);
        }
        s
    } else if has_suffix(&s, "ed") {
        step1b_tail(s, n - 2, false)
    } else if has_suffix(&s, "ing") {
        step1b_tail(s, n - 3, true)
    } else {
        s
    }
}

fn step1c(mut s: Vec<char>) -> Vec<char> {
    if s.len() < 2 {
        return s;
    }
    if let Some((last, before)) = s.split_last_mut() {
        if *last == 'y' && contains_vowel(before) {
            *last = 'i';
        } else if *last == 'Y' && contains_vowel(before) {
            *last = 'I';
        }
    }
    s
}

/// One rule of steps 2 to 4: when `s` ends with `suffix` and the measure of the stem before it passes `m_ok`,
/// `edit` rewrites `s`. The first matching suffix decides, whether or not its measure passes.
struct Rule {
    suffix: &'static str,
    edit: fn(&mut Vec<char>),
}

fn apply(s: Vec<char>, rules: &[Rule], m_ok: fn(usize) -> bool) -> Vec<char> {
    let mut s = s;
    for r in rules {
        if has_suffix(&s, r.suffix) {
            let stem = s.get(..s.len() - r.suffix.chars().count());
            if stem.is_some_and(|stem| m_ok(measure(stem))) {
                (r.edit)(&mut s);
            }
            return s;
        }
    }
    s
}

fn cut(s: &mut Vec<char>, n: usize) {
    s.truncate(s.len() - n);
}

/// Sets the letter `from_end` places from the end, 1 the last; every rule's suffix is that long.
fn set(s: &mut [char], from_end: usize, c: char) {
    if let Some(slot) = s.len().checked_sub(from_end).and_then(|i| s.get_mut(i)) {
        *slot = c;
    }
}

/// Step 2's rules, each a suffix and its edit, tried in order.
const STEP2_RULES: &[Rule] = &[
    Rule {
        suffix: "ational",
        edit: |s| {
            set(s, 5, 'e');
            cut(s, 4)
        },
    },
    Rule {
        suffix: "tional",
        edit: |s| cut(s, 2),
    },
    Rule {
        suffix: "enci",
        edit: |s| set(s, 1, 'e'),
    },
    Rule {
        suffix: "anci",
        edit: |s| set(s, 1, 'e'),
    },
    Rule {
        suffix: "izer",
        edit: |s| cut(s, 1),
    },
    Rule {
        suffix: "bli",
        edit: |s| set(s, 1, 'e'),
    },
    Rule {
        suffix: "alli",
        edit: |s| cut(s, 2),
    },
    Rule {
        suffix: "entli",
        edit: |s| cut(s, 2),
    },
    Rule {
        suffix: "eli",
        edit: |s| cut(s, 2),
    },
    Rule {
        suffix: "ousli",
        edit: |s| cut(s, 2),
    },
    Rule {
        suffix: "ization",
        edit: |s| {
            set(s, 5, 'e');
            cut(s, 4)
        },
    },
    Rule {
        suffix: "ation",
        edit: |s| {
            set(s, 3, 'e');
            cut(s, 2)
        },
    },
    Rule {
        suffix: "ator",
        edit: |s| {
            set(s, 2, 'e');
            cut(s, 1)
        },
    },
    Rule {
        suffix: "alism",
        edit: |s| cut(s, 3),
    },
    Rule {
        suffix: "iveness",
        edit: |s| cut(s, 4),
    },
    Rule {
        suffix: "fulness",
        edit: |s| cut(s, 4),
    },
    Rule {
        suffix: "ousness",
        edit: |s| cut(s, 4),
    },
    Rule {
        suffix: "aliti",
        edit: |s| cut(s, 3),
    },
    Rule {
        suffix: "iviti",
        edit: |s| {
            set(s, 3, 'e');
            cut(s, 2)
        },
    },
    Rule {
        suffix: "biliti",
        edit: |s| {
            set(s, 5, 'l');
            set(s, 4, 'e');
            cut(s, 3)
        },
    },
    Rule {
        suffix: "logi",
        edit: |s| cut(s, 1),
    },
];

fn step2(s: Vec<char>) -> Vec<char> {
    apply(s, STEP2_RULES, |m| m > 0)
}

fn step3(s: Vec<char>) -> Vec<char> {
    const RULES: &[Rule] = &[
        Rule {
            suffix: "icate",
            edit: |s| cut(s, 3),
        },
        Rule {
            suffix: "ative",
            edit: |s| cut(s, 5),
        },
        Rule {
            suffix: "alize",
            edit: |s| cut(s, 3),
        },
        Rule {
            suffix: "iciti",
            edit: |s| cut(s, 3),
        },
        Rule {
            suffix: "ical",
            edit: |s| cut(s, 2),
        },
        Rule {
            suffix: "ful",
            edit: |s| cut(s, 3),
        },
        Rule {
            suffix: "ness",
            edit: |s| cut(s, 4),
        },
    ];
    apply(s, RULES, |m| m > 0)
}

fn step4(s: Vec<char>) -> Vec<char> {
    const SUFFIXES: &[&str] = &[
        "al", "ance", "ence", "er", "ic", "able", "ible", "ant", "ement", "ment", "ent", "ion",
        "ou", "ism", "ate", "iti", "ous", "ive", "ize",
    ];
    let mut s = s;
    for suffix in SUFFIXES {
        if has_suffix(&s, suffix) {
            let sub_len = s.len() - suffix.len();
            let ok = s.get(..sub_len).is_some_and(|stem| {
                let m = measure(stem);
                if *suffix == "ion" {
                    m > 1 && matches!(stem.last(), Some('s' | 't'))
                } else {
                    m > 1
                }
            });
            if ok {
                s.truncate(sub_len);
            }
            return s;
        }
    }
    s
}

fn step5a(mut s: Vec<char>) -> Vec<char> {
    let drop_e = match s.split_last() {
        Some((&'e', sub)) => {
            let m = measure(sub);
            // A stem of measure 1 holds a vowel and a consonant after it, so it has a last letter.
            m > 1
                || (m == 1
                    && !(has_consonant_vowel_consonant_suffix(sub)
                        && !matches!(sub.last(), Some('w' | 'x' | 'y'))))
        }
        _ => false,
    };
    if drop_e {
        let _last: Option<char> = s.pop();
    }
    s
}

fn step5b(mut s: Vec<char>) -> Vec<char> {
    let drop_l = matches!(s.as_slice(), [_, .., 'l', 'l'])
        && s.split_last()
            .is_some_and(|(_, before)| measure(before) > 1);
    if drop_l {
        let _last: Option<char> = s.pop();
    }
    s
}

/// go-porterstemmer's `StemWithoutLowerCasing`.
fn stem_without_lower_casing(s: Vec<char>) -> Vec<char> {
    if s.len() <= 2 {
        return s;
    }
    let s = step1a(s);
    let s = step1b(s);
    let s = step1c(s);
    let s = step2(s);
    let s = step3(s);
    let s = step4(s);
    let s = step5a(s);
    step5b(s)
}
