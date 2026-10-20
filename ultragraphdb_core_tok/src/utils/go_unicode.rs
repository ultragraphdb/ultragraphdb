/*
 * Copyright 2009 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from the Go standard library, unicode (IsLetter). Changed for ultragraphdb: translated to Rust over the
 * table tools/golden/tok_tables generates from the Go toolchain building Dgraph.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::tables::unicode_tables::LETTER;

/// Go's `unicode.IsLetter`: the rune is in category L.
pub(crate) fn is_letter(c: char) -> bool {
    let r = u32::from(c);
    LETTER
        .binary_search_by(|&(lo, hi)| {
            if hi < r {
                std::cmp::Ordering::Less
            } else if lo > r {
                std::cmp::Ordering::Greater
            } else {
                std::cmp::Ordering::Equal
            }
        })
        .is_ok()
}
