/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use crate::bleve::{
    cjk_bigram, hindi_stemmer_filter, light_stemmer_de, light_stemmer_es, light_stemmer_fr,
    light_stemmer_it, light_stemmer_pt, porterstemmer, snowball, sorani_stemmer_filter, stemmer_ar,
};
use crate::types::analysis_token::AnalysisToken;
use crate::types::stemmer::{Kind, Stemmer};

impl Stemmer {
    /// The filter's `Filter`: each token's term stemmed, or, for `cjk_bigram`, the CJK bigrams of the stream.
    pub fn filter(&self, input: Vec<AnalysisToken>) -> Vec<AnalysisToken> {
        if self.0 == Kind::CjkBigram {
            return cjk_bigram::filter(input);
        }
        input
            .into_iter()
            .map(|mut t| {
                let term = self.stem(t.term());
                t.set_term(term);
                t
            })
            .collect()
    }

    /// The stem of one term; `cjk_bigram` keeps it.
    pub fn stem(&self, term: &[u8]) -> Vec<u8> {
        match self.0 {
            Kind::Porter => porterstemmer::stem(term),
            Kind::Arabic => stemmer_ar::stem(term),
            Kind::Sorani => sorani_stemmer_filter::stem(term),
            Kind::Hindi => hindi_stemmer_filter::stem(term),
            Kind::GermanLight => light_stemmer_de::stem(term),
            Kind::SpanishLight => light_stemmer_es::stem(term),
            Kind::FrenchLight => light_stemmer_fr::stem(term),
            Kind::ItalianLight => light_stemmer_it::stem(term),
            Kind::PortugueseLight => light_stemmer_pt::stem(term),
            Kind::Snowball(a) => snowball::stem(a, term),
            Kind::CjkBigram => term.to_vec(),
        }
    }
}
