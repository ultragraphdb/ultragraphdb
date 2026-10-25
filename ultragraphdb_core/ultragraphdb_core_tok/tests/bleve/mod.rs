/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_tok::Stemmer;

#[cfg(test)]
mod cjk_bigram_tests;
#[cfg(test)]
mod hindi_stemmer_filter_tests;
#[cfg(test)]
mod light_stemmer_de_tests;
#[cfg(test)]
mod light_stemmer_es_tests;
#[cfg(test)]
mod light_stemmer_fr_tests;
#[cfg(test)]
mod light_stemmer_it_tests;
#[cfg(test)]
mod light_stemmer_pt_tests;
#[cfg(test)]
mod lowercase_tests;
#[cfg(test)]
mod porterstemmer_tests;
#[cfg(test)]
mod segment_words_tests;
#[cfg(test)]
mod snowball_tests;
#[cfg(test)]
mod sorani_stemmer_filter_tests;
#[cfg(test)]
mod stemmer_ar_tests;
#[cfg(test)]
mod stop_tests;
#[cfg(test)]
mod unicode_tokenizer_tests;
#[cfg(test)]
mod unicodenorm_tests;

/// Terms at the edges of every stemmer: empty, one and two runes, a decomposed accent, a lone combining mark and a
/// lone zero-width non-joiner, runs of `y`, bare suffixes, and short Arabic, Devanagari and Sorani affixes. The
/// golden words of `tok_stemmers` have no empty term and build their affixed words on bases of three runes or more.
pub(crate) const EDGE_INPUTS: [&str; 24] = [
    "", "a", "y", "é", "É", "e\u{301}", "ab", "aé", "abc", "yyyy", "ayyy", "123", "e", "es", "ies",
    "ing", "eed", "\u{200c}", "\u{300}", "日本", "ال", "و", "का", "هە",
];

/// Checks the stemmer `name` against Go: each of `cases` stems as bleve's filter stems it; each of [`EDGE_INPUTS`]
/// stems to its entry in `changed`, or stays as it is; and a 300-letter term stays as it is. The expected values are
/// bleve v2.5.7's, from its tests or from running its filter.
pub(crate) fn check_stems(name: &str, cases: &[(&str, &str)], changed: &[(&str, &str)]) {
    let s = Stemmer::named(name).unwrap_or_else(|| panic!("no stemmer {name}"));
    let mut failures = Vec::new();
    for &(input, want) in cases {
        if s.stem(input) != want {
            failures.push(format!("{input:?}: want {want:?}, got {:?}", s.stem(input)));
        }
    }
    for input in EDGE_INPUTS {
        let want = changed
            .iter()
            .find(|(i, _)| *i == input)
            .map_or(input, |&(_, o)| o);
        if s.stem(input) != want {
            failures.push(format!("{input:?}: want {want:?}, got {:?}", s.stem(input)));
        }
    }
    assert!(
        changed.iter().all(|(i, _)| EDGE_INPUTS.contains(i)),
        "{name}: {changed:?} names a term that is not an edge input"
    );
    let long = "abcdefghij".repeat(30);
    assert_eq!(s.stem(&long), long, "{name}: a 300-letter term");
    assert!(failures.is_empty(), "{name}:\n{}", failures.join("\n"));
}
