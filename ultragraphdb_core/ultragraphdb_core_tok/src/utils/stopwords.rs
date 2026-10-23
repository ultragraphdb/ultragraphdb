/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, tok/stopwords.go. Changed for ultragraphdb: translated to Rust over the stop-word
 * tables of bleve v2.5.7.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::bleve::stop;
use crate::types::analysis_token::AnalysisToken;

/// Dgraph's `filterStopwords`: the tokens of `input` that are not stop words of `lang`, a language base among
/// Dgraph's 28 (`ar`, `bg`, `ca`, `ckb`, `cs`, `da`, `de`, `el`, `en`, `es`, `eu`, `fa`, `fi`, `fr`, `ga`, `gl`,
/// `hi`, `hu`, `hy`, `id`, `it`, `nl`, `no`, `pt`, `ro`, `ru`, `sv`, `tr`); the input unchanged for any other.
pub fn filter_stopwords(lang: &str, input: Vec<AnalysisToken>) -> Vec<AnalysisToken> {
    if input.is_empty() {
        return input;
    }
    match stop::stop_words(lang) {
        Some(words) => stop::filter(words, input),
        None => input,
    }
}

/// The stop words of `lang`, sorted bytewise, as bleve's token map for it holds them.
pub fn stop_words(lang: &str) -> Option<&'static [&'static str]> {
    stop::stop_words(lang)
}
