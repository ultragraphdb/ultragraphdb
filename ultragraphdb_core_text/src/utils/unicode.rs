/*
 * Copyright 2009 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from the Go standard library, unicode/letter.go (Is over a range table). Changed for ultragraphdb:
 * translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::tables::unicode_tables::{IDEOGRAPHIC, ND};

fn is_in(table: &[(u32, u32, u32)], r: u32) -> bool {
    let i = table.partition_point(|&(_, hi, _)| hi < r);
    match table.get(i) {
        Some(&(lo, _, stride)) => r >= lo && (r - lo).is_multiple_of(stride),
        None => false,
    }
}

/// Go's `unicode.Is(unicode.Ideographic, r)`.
pub(crate) fn is_ideographic(r: u32) -> bool {
    is_in(&IDEOGRAPHIC, r)
}

/// Go's `unicode.In(r, unicode.Nd)`.
pub(crate) fn is_decimal_digit(r: u32) -> bool {
    is_in(&ND, r)
}
