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

/// A Snowball stemmer's `Stem` on one term.
pub(crate) fn stem(algorithm: Algorithm, term: &str) -> String {
    Stemmer::create(algorithm).stem(term).into_owned()
}
