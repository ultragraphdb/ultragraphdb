/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_core_tok::stop_words;

/// bleve v2.5.7's token map `stop_<lang>` for each language of Dgraph's `langStops`, as a Go program over bleve's
/// registry reads it: the number of words, their total length in bytes, and the first and last word sorted
/// bytewise.
pub(crate) const GO_STOP_WORDS: [(&str, usize, usize, &str, &str); 28] = [
    ("ar", 119, 704, "أ", "يكون"),
    ("bg", 190, 1466, "а", "я"),
    ("ca", 218, 985, "a", "és"),
    ("ckb", 62, 518, "ئێمە", "ی"),
    ("cs", 171, 685, "a", "články"),
    ("da", 94, 330, "ad", "været"),
    ("de", 231, 1118, "aber", "über"),
    ("el", 75, 574, "αλλα", "ωσ"),
    ("en", 174, 780, "a", "yourselves"),
    ("es", 308, 1822, "a", "éramos"),
    ("eu", 98, 478, "al", "zuten"),
    ("fa", 308, 2370, "اباد", "گيري"),
    ("fi", 229, 1312, "ei", "yli"),
    ("fr", 164, 673, "ai", "êtes"),
    ("ga", 109, 443, "a", "ónár"),
    ("gl", 160, 639, "a", "ós"),
    ("hi", 225, 2439, "अंदर", "होने"),
    ("hu", 198, 1024, "a", "őket"),
    ("hy", 45, 252, "այդ", "և"),
    ("id", 355, 2325, "ada", "yang"),
    ("it", 279, 1375, "a", "è"),
    ("nl", 101, 352, "aan", "zou"),
    ("no", 172, 660, "alle", "å"),
    ("pt", 203, 1064, "a", "éramos"),
    ("ro", 230, 1105, "acea", "ţie"),
    ("ru", 159, 1190, "а", "я"),
    ("sv", 114, 445, "alla", "över"),
    ("tr", 209, 1152, "acaba", "şöyle"),
];

#[test]
fn test_stop_word_tables_match_bleve() {
    for (lang, count, bytes, first, last) in GO_STOP_WORDS {
        let words = stop_words(lang).unwrap_or_else(|| panic!("no stop words for {lang}"));
        assert_eq!(words.len(), count, "{lang}: count");
        assert_eq!(
            words.iter().map(|w| w.len()).sum::<usize>(),
            bytes,
            "{lang}: bytes"
        );
        assert_eq!((words[0], words[words.len() - 1]), (first, last), "{lang}");
        // Strictly ascending: sorted for the binary search, and no word twice, as in a Go map.
        assert!(
            words.windows(2).all(|w| w[0] < w[1]),
            "{lang}: not strictly sorted"
        );
        assert!(!words.contains(&""), "{lang}: an empty word");
    }
}

#[test]
fn test_stop_word_tables_hold_dgraph_languages_only() {
    // Languages Dgraph stems or tokenizes but keeps no stop words for, a region subtag, a case variant, and the empty
    // tag: Dgraph's langStops has no key for them.
    for lang in ["zh", "ja", "ko", "th", "en-US", "EN", "", "xx", "aa", "zz"] {
        assert_eq!(stop_words(lang), None, "{lang:?}");
    }
}
