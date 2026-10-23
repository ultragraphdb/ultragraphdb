/*
 * Copyright (c) 2013 Charles Iliya Krempeaux <charles@reptile.ca> :: http://changelog.ca/
 * SPDX-License-Identifier: MIT
 *
 * Ported from github.com/blevesearch/go-porterstemmer v1.0.3, porterstemmer.go (StemWithoutLowerCasing), as
 * bleve v2.5.7's analysis/token/porter/porter.go calls it. Changed for ultragraphdb: translated to Rust. The
 * MIT notice is in LICENSE-PORTERSTEMMER.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_common_utils::go_utf8;

/// bleve's `stemmer_porter` on one term: go-porterstemmer's `StemWithoutLowerCasing` over its runes.
pub(crate) fn stem(term: &[u8]) -> Vec<u8> {
    go_utf8::encode(&stem_without_lower_casing(go_utf8::runes(term)))
}

fn is_consonant(s: &[char], i: usize) -> bool {
    match s[i] {
        'a' | 'e' | 'i' | 'o' | 'u' => false,
        'y' => i == 0 || !is_consonant(s, i - 1),
        _ => true,
    }
}

fn measure(s: &[char]) -> u32 {
    let len = s.len();
    let mut result = 0;
    let mut i = 0;
    if len == 0 {
        return result;
    }
    while is_consonant(s, i) {
        i += 1;
        if i >= len {
            return result;
        }
    }
    'outer: while i < len {
        while !is_consonant(s, i) {
            i += 1;
            if i >= len {
                break 'outer;
            }
        }
        while is_consonant(s, i) {
            i += 1;
            if i >= len {
                result += 1;
                break 'outer;
            }
        }
        result += 1;
    }
    result
}

/// go-porterstemmer's `hasSuffix`: `s` ends with `suffix` and is longer than it.
fn has_suffix(s: &[char], suffix: &str) -> bool {
    let suffix: Vec<char> = suffix.chars().collect();
    s.len() > suffix.len() && s.ends_with(&suffix)
}

fn contains_vowel(s: &[char]) -> bool {
    (0..s.len()).any(|i| !is_consonant(s, i))
}

fn has_repeat_double_consonant_suffix(s: &[char]) -> bool {
    let n = s.len();
    n >= 2 && s[n - 1] == s[n - 2] && is_consonant(s, n - 1)
}

fn has_consonant_vowel_consonant_suffix(s: &[char]) -> bool {
    let n = s.len();
    n >= 3 && is_consonant(s, n - 3) && !is_consonant(s, n - 2) && is_consonant(s, n - 1)
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
    let sub = s[..sub_len].to_vec();
    if !contains_vowel(&sub) {
        return s;
    }
    if has_suffix(&sub, "at") || has_suffix(&sub, "bl") || has_suffix(&sub, "iz") {
        s.truncate(sub_len + 1);
        if at_bl_iz_e {
            s[sub_len] = 'e';
        }
        return s;
    }
    let c = sub[sub.len() - 1];
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
        s[sub_len] = 'e';
        return s;
    }
    s.truncate(sub_len);
    s
}

fn step1b(mut s: Vec<char>) -> Vec<char> {
    let n = s.len();
    if has_suffix(&s, "eed") {
        if measure(&s[..n - 3]) > 0 {
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
    let n = s.len();
    if n < 2 {
        return s;
    }
    if s[n - 1] == 'y' && contains_vowel(&s[..n - 1]) {
        s[n - 1] = 'i';
    } else if s[n - 1] == 'Y' && contains_vowel(&s[..n - 1]) {
        s[n - 1] = 'I';
    }
    s
}

/// One rule of steps 2 to 4: when `s` ends with `suffix` and the measure of the stem before it passes `m_ok`,
/// `edit` rewrites `s`. The first matching suffix decides, whether or not its measure passes.
struct Rule {
    suffix: &'static str,
    edit: fn(&mut Vec<char>),
}

fn apply(s: Vec<char>, rules: &[Rule], m_ok: fn(u32) -> bool) -> Vec<char> {
    let mut s = s;
    for r in rules {
        if has_suffix(&s, r.suffix) {
            if m_ok(measure(&s[..s.len() - r.suffix.chars().count()])) {
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

fn step2(s: Vec<char>) -> Vec<char> {
    const RULES: &[Rule] = &[
        Rule {
            suffix: "ational",
            edit: |s| {
                let n = s.len();
                s[n - 5] = 'e';
                cut(s, 4)
            },
        },
        Rule {
            suffix: "tional",
            edit: |s| cut(s, 2),
        },
        Rule {
            suffix: "enci",
            edit: |s| {
                let n = s.len();
                s[n - 1] = 'e'
            },
        },
        Rule {
            suffix: "anci",
            edit: |s| {
                let n = s.len();
                s[n - 1] = 'e'
            },
        },
        Rule {
            suffix: "izer",
            edit: |s| cut(s, 1),
        },
        Rule {
            suffix: "bli",
            edit: |s| {
                let n = s.len();
                s[n - 1] = 'e'
            },
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
                let n = s.len();
                s[n - 5] = 'e';
                cut(s, 4)
            },
        },
        Rule {
            suffix: "ation",
            edit: |s| {
                let n = s.len();
                s[n - 3] = 'e';
                cut(s, 2)
            },
        },
        Rule {
            suffix: "ator",
            edit: |s| {
                let n = s.len();
                s[n - 2] = 'e';
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
                let n = s.len();
                s[n - 3] = 'e';
                cut(s, 2)
            },
        },
        Rule {
            suffix: "biliti",
            edit: |s| {
                let n = s.len();
                s[n - 5] = 'l';
                s[n - 4] = 'e';
                cut(s, 3)
            },
        },
        Rule {
            suffix: "logi",
            edit: |s| cut(s, 1),
        },
    ];
    apply(s, RULES, |m| m > 0)
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
            let m = measure(&s[..sub_len]);
            let ok = if *suffix == "ion" {
                m > 1 && matches!(s[sub_len - 1], 's' | 't')
            } else {
                m > 1
            };
            if ok {
                s.truncate(sub_len);
            }
            return s;
        }
    }
    s
}

fn step5a(mut s: Vec<char>) -> Vec<char> {
    let n = s.len();
    if s[n - 1] == 'e' {
        let sub = &s[..n - 1];
        let m = measure(sub);
        if m > 1 {
            s.truncate(n - 1);
        } else if m == 1 {
            let c = sub[sub.len() - 1];
            if !(has_consonant_vowel_consonant_suffix(sub) && c != 'w' && c != 'x' && c != 'y') {
                s.truncate(n - 1);
            }
        }
    }
    s
}

fn step5b(mut s: Vec<char>) -> Vec<char> {
    let n = s.len();
    if n > 2 && s[n - 2] == 'l' && s[n - 1] == 'l' && measure(&s[..n - 1]) > 1 {
        s.truncate(n - 1);
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
