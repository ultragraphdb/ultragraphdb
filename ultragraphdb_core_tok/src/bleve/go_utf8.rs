/*
 * Copyright 2009 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from the Go standard library, unicode/utf8 (DecodeRune, EncodeRune) and bytes (Runes), as bleve and
 * blevesearch/segment call them. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

/// Go's `utf8.RuneError`, U+FFFD.
pub(crate) const RUNE_ERROR: u32 = 0xFFFD;

/// Go's `utf8.DecodeRune`: the first rune of `b` and its width; `(RUNE_ERROR, 1)` for an invalid or incomplete
/// encoding, `(RUNE_ERROR, 0)` for empty input.
pub(crate) fn decode_rune(b: &[u8]) -> (u32, usize) {
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
pub(crate) fn runes(b: &[u8]) -> Vec<char> {
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
pub(crate) fn encode(runes: &[char]) -> Vec<u8> {
    runes.iter().collect::<String>().into_bytes()
}
