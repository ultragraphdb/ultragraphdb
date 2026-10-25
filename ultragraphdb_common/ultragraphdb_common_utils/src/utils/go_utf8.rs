/*
 * Copyright 2009 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from Go 1.27, src/unicode/utf8 (DecodeRune, EncodeRune) and src/bytes (Runes), as bleve and
 * blevesearch/segment call them, and the conversion of Go's bytes to runes. Changed for ultragraphdb: translated to
 * Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

//! Go's `unicode/utf8` and how Go reads bytes as runes: a Go string holds any bytes, and every byte that is not
//! part of a UTF-8 sequence reads as one U+FFFD.

use std::borrow::Cow;

/// Go's `utf8.RuneError`, U+FFFD.
pub const RUNE_ERROR: u32 = 0xFFFD;

/// Go's `utf8.DecodeRune`: the first rune of `b` and its width; `(RUNE_ERROR, 1)` for an invalid or incomplete
/// encoding, `(RUNE_ERROR, 0)` for empty input.
pub fn decode_rune(b: &[u8]) -> (u32, usize) {
    let Some(&b0) = b.first() else {
        return (RUNE_ERROR, 0);
    };
    let cont = |i: usize, lo: u8, hi: u8| {
        b.get(i)
            .filter(|&&c| (lo..=hi).contains(&c))
            .map(|&c| u32::from(c & 0x3F))
    };
    match b0 {
        0x00..=0x7F => (u32::from(b0), 1),
        0xC2..=0xDF => match cont(1, 0x80, 0xBF) {
            Some(c1) => ((u32::from(b0 & 0x1F) << 6) | c1, 2),
            None => (RUNE_ERROR, 1),
        },
        0xE0..=0xEF => {
            let (lo, hi) = match b0 {
                0xE0 => (0xA0, 0xBF),
                0xED => (0x80, 0x9F),
                _ => (0x80, 0xBF),
            };
            match (cont(1, lo, hi), cont(2, 0x80, 0xBF)) {
                (Some(c1), Some(c2)) => ((u32::from(b0 & 0x0F) << 12) | (c1 << 6) | c2, 3),
                _ => (RUNE_ERROR, 1),
            }
        }
        0xF0..=0xF4 => {
            let (lo, hi) = match b0 {
                0xF0 => (0x90, 0xBF),
                0xF4 => (0x80, 0x8F),
                _ => (0x80, 0xBF),
            };
            match (cont(1, lo, hi), cont(2, 0x80, 0xBF), cont(3, 0x80, 0xBF)) {
                (Some(c1), Some(c2), Some(c3)) => (
                    (u32::from(b0 & 0x07) << 18) | (c1 << 12) | (c2 << 6) | c3,
                    4,
                ),
                _ => (RUNE_ERROR, 1),
            }
        }
        _ => (RUNE_ERROR, 1),
    }
}

/// Go's `bytes.Runes`: the runes of `b`, each invalid byte as U+FFFD.
pub fn runes(b: &[u8]) -> Vec<char> {
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let (r, n) = decode_rune(&b[i..]);
        out.push(char::from_u32(r).unwrap_or(char::REPLACEMENT_CHARACTER));
        i += n;
    }
    out
}

/// Go's `string(runes)` and bleve's `BuildTermFromRunes`: the UTF-8 encoding of `runes`.
pub fn encode(runes: &[char]) -> Vec<u8> {
    runes.iter().collect::<String>().into_bytes()
}

/// The bytes as text, each byte that is not part of a UTF-8 sequence replaced by U+FFFD. That is how Go reads them:
/// ranging over a string, decoding a rune, unquoting a literal or writing JSON, Go takes every such byte for one
/// U+FFFD (`String::from_utf8_lossy` replaces a truncated sequence by a single one instead).
pub fn go_string(b: &[u8]) -> Cow<'_, str> {
    if let Ok(s) = std::str::from_utf8(b) {
        return Cow::Borrowed(s);
    }
    let mut out = String::with_capacity(b.len() + 8);
    for chunk in b.utf8_chunks() {
        out.push_str(chunk.valid());
        chunk.invalid().iter().for_each(|_| out.push('\u{fffd}'));
    }
    Cow::Owned(out)
}
