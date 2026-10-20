/*
 * Copyright (c) 2014 Couchbase, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from bleve v2.5.7, analysis/type.go (TokenType). Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

/// bleve's `TokenType`, with its numeric values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TokenType {
    /// Letters and digits.
    AlphaNumeric = 0,
    /// Han, Hiragana and Katakana.
    Ideographic = 1,
    /// A number.
    Numeric = 2,
    /// A date.
    DateTime = 3,
    /// A shingle.
    Shingle = 4,
    /// A unigram the CJK bigram filter emits.
    Single = 5,
    /// A bigram the CJK bigram filter emits.
    Double = 6,
    /// A boolean.
    Boolean = 7,
    /// An IP address.
    Ip = 8,
}
