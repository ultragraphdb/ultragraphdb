/*
 * Copyright (c) 2015 Couchbase, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from github.com/blevesearch/segment v0.9.1, segment_words.go (the word types). Changed for
 * ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

/// segment's word types: what a word segment holds. bleve's unicode tokenizer keeps every segment but `None`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SegmentType {
    /// Not a word: spaces, punctuation, symbols, line breaks.
    None = 0,
    /// A number.
    Number = 1,
    /// Letters, Hangul included.
    Letter = 2,
    /// Kana; segment v0.9.1 never produces it, typing Katakana and Hiragana as `Ideo`.
    Kana = 3,
    /// Han, Hiragana or Katakana.
    Ideo = 4,
}
