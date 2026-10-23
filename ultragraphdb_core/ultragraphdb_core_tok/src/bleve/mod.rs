/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

//! The pieces of bleve v2.5.7 and its libraries that Dgraph's analysis runs, one module per Go file.

pub(crate) mod cjk_bigram;
pub(crate) mod hindi_stemmer_filter;
pub(crate) mod light_stemmer_de;
pub(crate) mod light_stemmer_es;
pub(crate) mod light_stemmer_fr;
pub(crate) mod light_stemmer_it;
pub(crate) mod light_stemmer_pt;
pub(crate) mod lowercase;
pub(crate) mod porterstemmer;
pub(crate) mod segment_words;
pub(crate) mod snowball;
pub(crate) mod sorani_stemmer_filter;
pub(crate) mod stemmer_ar;
pub(crate) mod stop;
pub(crate) mod unicode_tokenizer;
pub(crate) mod unicodenorm;
