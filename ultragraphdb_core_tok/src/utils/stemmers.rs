/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, tok/stemmers.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::types::analysis_token::AnalysisToken;
use crate::types::stemmer::Stemmer;

/// Dgraph's `filterStemmers`: `input` through the stemmer of `lang`, a language base; the input unchanged when the
/// language has none.
pub fn filter_stemmers(lang: &str, input: Vec<AnalysisToken>) -> Vec<AnalysisToken> {
    if input.is_empty() {
        return input;
    }
    match Stemmer::for_lang(lang) {
        Some(s) => s.filter(input),
        None => input,
    }
}
