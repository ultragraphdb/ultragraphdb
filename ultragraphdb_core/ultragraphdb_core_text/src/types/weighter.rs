/*
 * Copyright 2013 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from golang.org/x/text v0.41.0, internal/colltab/weighter.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::types::elem::Elem;
use crate::types::numeric_weighter::NumericWeighter;
use crate::types::table::Table;

/// The source of collation elements: a locale's table, or the table wrapped to weigh numbers by value. An enum
/// in place of Go's `Weighter` interface, for the two implementations a collator uses.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Weighter {
    Table(Table),
    Numeric(NumericWeighter),
}

impl Weighter {
    /// Appends the elements of the longest match of a single character or contraction at the start of `s` and
    /// returns the number of bytes consumed.
    pub(crate) fn append_next(&self, w: &mut Vec<Elem>, s: &[u8]) -> usize {
        match self {
            Weighter::Table(t) => t.append_next(w, s),
            Weighter::Numeric(n) => n.append_next(w, s),
        }
    }

    /// The highest variable primary weight.
    pub(crate) fn top(&self) -> u32 {
        match self {
            Weighter::Table(t) => t.top(),
            Weighter::Numeric(n) => n.top(),
        }
    }
}
