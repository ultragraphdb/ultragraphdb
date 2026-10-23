/*
 * Copyright (c) 2014 Couchbase, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from bleve v2.5.7, analysis/lang/ckb/sorani_stemmer_filter.go. Changed for ultragraphdb: translated to
 * Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

/// bleve's `stemmer_ckb` on one term.
pub(crate) fn stem(term: &str) -> String {
    let mut input = term.to_string();
    let mut len = input.chars().count();
    let ends = |b: &str, s: &str| b.ends_with(s);

    // postposition
    if len > 5 && ends(&input, "دا") {
        input = truncate_runes(&input, 2);
        len = input.chars().count();
    } else if len > 4 && ends(&input, "نا") {
        input = truncate_runes(&input, 1);
        len = input.chars().count();
    } else if len > 6 && ends(&input, "ەوە") {
        input = truncate_runes(&input, 3);
        len = input.chars().count();
    }

    // possessive pronoun
    if len > 6 && (ends(&input, "مان") || ends(&input, "یان") || ends(&input, "تان")) {
        input = truncate_runes(&input, 3);
        len = input.chars().count();
    }

    // indefinite singular ezafe
    if len > 6 && ends(&input, "ێکی") {
        return truncate_runes(&input, 3);
    } else if len > 7 && ends(&input, "یەکی") {
        return truncate_runes(&input, 4);
    }

    const RULES: [(usize, &str, usize); 15] = [
        (5, "ێک", 2),   // indefinite singular
        (6, "یەک", 3),  // indefinite singular
        (6, "ەکە", 3),  // definite singular
        (5, "کە", 2),   // definite singular
        (7, "ەکان", 4), // definite plural
        (6, "کان", 3),  // definite plural
        (7, "یانی", 4), // indefinite plural ezafe
        (6, "انی", 3),  // indefinite plural ezafe
        (6, "یان", 3),  // indefinite plural
        (5, "ان", 2),   // indefinite plural
        (7, "یانە", 4), // demonstrative plural
        (6, "انە", 3),  // demonstrative plural
        (5, "ایە", 2),  // demonstrative singular
        (5, "ەیە", 2),  // demonstrative singular
        (4, "ە", 1),    // demonstrative singular
    ];
    for (min, suffix, n) in RULES {
        if len > min && ends(&input, suffix) {
            return truncate_runes(&input, n);
        }
    }
    if len > 4 && ends(&input, "ی") {
        // absolute singular ezafe
        return truncate_runes(&input, 1);
    }
    input
}

/// bleve's `truncateRunes`.
fn truncate_runes(input: &str, num: usize) -> String {
    let r: Vec<char> = input.chars().collect();
    r[..r.len() - num].iter().collect()
}
