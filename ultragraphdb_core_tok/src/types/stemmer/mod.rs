/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, tok/stemmers.go (langStemmers). Changed for ultragraphdb: translated to Rust; the
 * bleve token filters Dgraph looks up by name, as one type.
 * Copyright (c) 2026. Marvin Hansen.
 */

mod stemmer_filter;

use rust_stemmers::Algorithm;

/// A stemmer token filter of bleve v2.5.7 that Dgraph's `langStemmers` names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stemmer(Kind);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Porter,
    Arabic,
    Sorani,
    Hindi,
    GermanLight,
    SpanishLight,
    FrenchLight,
    ItalianLight,
    PortugueseLight,
    Snowball(Algorithm),
    CjkBigram,
}

/// Dgraph's `langStemmers`: language base, bleve filter name, filter. Sorted by language.
const LANG_STEMMERS: [(&str, &str, Kind); 21] = [
    ("ar", "stemmer_ar", Kind::Arabic),
    ("ckb", "stemmer_ckb", Kind::Sorani),
    (
        "da",
        "stemmer_da_snowball",
        Kind::Snowball(Algorithm::Danish),
    ),
    ("de", "stemmer_de_light", Kind::GermanLight),
    ("en", "stemmer_porter", Kind::Porter),
    ("es", "stemmer_es_light", Kind::SpanishLight),
    (
        "fi",
        "stemmer_fi_snowball",
        Kind::Snowball(Algorithm::Finnish),
    ),
    ("fr", "stemmer_fr_light", Kind::FrenchLight),
    ("hi", "stemmer_hi", Kind::Hindi),
    (
        "hu",
        "stemmer_hu_snowball",
        Kind::Snowball(Algorithm::Hungarian),
    ),
    ("it", "stemmer_it_light", Kind::ItalianLight),
    ("ja", "cjk_bigram", Kind::CjkBigram),
    ("ko", "cjk_bigram", Kind::CjkBigram),
    (
        "nl",
        "stemmer_nl_snowball",
        Kind::Snowball(Algorithm::Dutch),
    ),
    (
        "no",
        "stemmer_no_snowball",
        Kind::Snowball(Algorithm::Norwegian),
    ),
    ("pt", "stemmer_pt_light", Kind::PortugueseLight),
    (
        "ro",
        "stemmer_ro_snowball",
        Kind::Snowball(Algorithm::Romanian),
    ),
    (
        "ru",
        "stemmer_ru_snowball",
        Kind::Snowball(Algorithm::Russian),
    ),
    (
        "sv",
        "stemmer_sv_snowball",
        Kind::Snowball(Algorithm::Swedish),
    ),
    (
        "tr",
        "stemmer_tr_snowball",
        Kind::Snowball(Algorithm::Turkish),
    ),
    ("zh", "cjk_bigram", Kind::CjkBigram),
];

impl Stemmer {
    /// The stemmer Dgraph uses for a language base, as `langStemmers` maps it.
    pub fn for_lang(lang: &str) -> Option<Stemmer> {
        LANG_STEMMERS
            .binary_search_by_key(&lang, |&(l, _, _)| l)
            .ok()
            .map(|i| Stemmer(LANG_STEMMERS[i].2))
    }

    /// The stemmer of a bleve filter name `langStemmers` uses, such as `stemmer_porter`.
    pub fn named(name: &str) -> Option<Stemmer> {
        LANG_STEMMERS
            .iter()
            .find(|&&(_, n, _)| n == name)
            .map(|&(_, _, k)| Stemmer(k))
    }

    /// The bleve filter name.
    pub fn name(&self) -> &'static str {
        LANG_STEMMERS
            .iter()
            .find(|&&(_, _, k)| k == self.0)
            .map(|&(_, n, _)| n)
            .expect("every stemmer is in LANG_STEMMERS")
    }
}
