/*
 * Copyright 2009 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from Go 1.27, src/unicode (IsLetter, IsDigit, Is over a range table, ToLower). Changed for
 * ultragraphdb: translated to Rust over the tables tools/golden/tok_tables and tools/golden/text_tables generate
 * from the Go toolchain building Dgraph.
 * Copyright (c) 2026. Marvin Hansen.
 */

//! Go's `unicode` as the port uses it, at the Unicode version of the Go toolchain building Dgraph. A rune is Go's:
//! any `u32` or `char`, and a value beyond Unicode belongs to no category and maps to itself.

use std::cmp::Ordering;

use crate::tables::unicode_range_tables::{IDEOGRAPHIC, ND};
use crate::tables::unicode_tables::{LETTER, TO_LOWER};

fn is_in(table: &[(u32, u32, u32)], r: u32) -> bool {
    let i = table.partition_point(|&(_, hi, _)| hi < r);
    match table.get(i) {
        Some(&(lo, _, stride)) => r >= lo && (r - lo).is_multiple_of(stride),
        None => false,
    }
}

/// Go's `unicode.IsLetter`: the rune is in category L.
pub fn is_letter<R: Into<u32>>(r: R) -> bool {
    let r = r.into();
    LETTER
        .binary_search_by(|&(lo, hi)| {
            if hi < r {
                Ordering::Less
            } else if lo > r {
                Ordering::Greater
            } else {
                Ordering::Equal
            }
        })
        .is_ok()
}

/// Go's `unicode.IsDigit`: the rune is in category Nd, `unicode.In(r, unicode.Nd)`.
pub fn is_digit<R: Into<u32>>(r: R) -> bool {
    is_in(&ND, r.into())
}

/// Go's `unicode.Is(unicode.Ideographic, r)`.
pub fn is_ideographic<R: Into<u32>>(r: R) -> bool {
    is_in(&IDEOGRAPHIC, r.into())
}

/// Go's `unicode.ToLower`: the rune's simple lowercase mapping, the rune itself when it has none.
pub fn to_lower<R: Into<u32>>(r: R) -> u32 {
    let r = r.into();
    if r < 0x80 {
        return if (u32::from(b'A')..=u32::from(b'Z')).contains(&r) {
            r + 32
        } else {
            r
        };
    }
    TO_LOWER
        .binary_search_by_key(&r, |&(from, _)| from)
        .map_or(r, |i| TO_LOWER[i].1)
}
