/*
 * Copyright (c) 2014 Couchbase, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from bleve v2.5.7, analysis/lang/{da,fi,hu,nl,no,ro,ru,sv,tr}/stemmer_*.go, which run the Snowball
 * stemmers of github.com/blevesearch/snowballstem v0.9.0. Changed for ultragraphdb: translated to Rust over
 * rust-stemmers, whose Snowball stemmers for these nine languages agree with snowballstem's on the golden vectors
 * (testdata/golden/tok_stemmers.json).
 * Copyright (c) 2026. Marvin Hansen.
 */

use rust_stemmers::{Algorithm, Stemmer};

/// A Snowball stemmer's `Stem` on one term. Terms reach the stemmers as the segmenter's word segments, which are
/// valid UTF-8; anything else is returned unchanged.
pub(crate) fn stem(algorithm: Algorithm, term: &[u8]) -> Vec<u8> {
    match std::str::from_utf8(term) {
        Ok(s) => Stemmer::create(algorithm).stem(s).into_owned().into_bytes(),
        Err(_) => term.to_vec(),
    }
}
