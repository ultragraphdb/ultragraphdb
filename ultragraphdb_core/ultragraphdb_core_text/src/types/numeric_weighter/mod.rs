/*
 * Copyright 2014 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from golang.org/x/text v0.41.0, internal/colltab/numeric.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use ultragraphdb_common_utils::go_unicode;

use crate::types::elem::{DEFAULT_SECONDARY, DEFAULT_TERTIARY, Elem, MAX_DIGITS};
use crate::types::table::Table;
use crate::utils::norm::first_rune;

/// Wraps a table so that a sequence of decimal digits (category Nd) sorts at the primary level by its numeric
/// value: "A-21" < "A-123". The digits of one kind are assumed to sort in order of their value, with the tertiary
/// order preserved.
#[derive(Debug, Clone, Copy)]
pub(crate) struct NumericWeighter {
    table: Table,
    /// Normal digit zero.
    zero: Elem,
    /// Special digit zero, low tertiary value.
    zero_special_lo: Elem,
    /// Special digit zero, high tertiary value.
    zero_special_hi: Elem,
    /// Normal digit nine.
    nine: Elem,
    /// Special digit nine.
    nine_special_hi: Elem,
    number_start: Elem,
}

impl NumericWeighter {
    pub(crate) fn new(table: Table) -> NumericWeighter {
        let get_elem = |s: &str| {
            let mut elems = Vec::new();
            table.append_next(&mut elems, s.as_bytes());
            elems[0]
        };
        let nine = get_elem("9");
        // Numbers should order before zero, but the DUCET has no room for this.
        let number_start = Elem::make(
            nine.primary() + 1,
            nine.secondary(),
            i32::from(nine.tertiary()),
            0,
        )
        .unwrap_or_default();
        NumericWeighter {
            table,
            zero: get_elem("0"),
            zero_special_lo: get_elem("\u{FF10}"), // FULLWIDTH DIGIT ZERO
            zero_special_hi: get_elem("\u{2080}"), // SUBSCRIPT ZERO
            nine,
            nine_special_hi: get_elem("\u{2089}"), // SUBSCRIPT NINE
            number_start,
        }
    }

    pub(crate) fn top(&self) -> u32 {
        self.table.top()
    }

    /// Appends the elements of the next rune, contraction or number in `s` to `w` and returns the number of
    /// bytes consumed.
    pub(crate) fn append_next(&self, w: &mut Vec<Elem>, s: &[u8]) -> usize {
        let old_len = w.len();
        let mut n = self.table.append_next(w, s);
        // Go's numberConverter.elems: the prefix of w that is kept.
        let mut kept = old_len;
        let Some(is_zero) = self.check_next_digit(kept, w, s) else {
            return n;
        };
        // Insert a marker indicating the start of a number and a placeholder for the number of digits.
        let mut n_digits = 0;
        if is_zero {
            w.truncate(old_len);
            w.push(self.number_start);
            w.push(Elem(0));
        } else {
            w.insert(old_len, Elem(0));
            w.insert(old_len, self.number_start);
            n_digits = 1;
        }
        kept = w.len();
        let len_index = old_len + 1;
        while n < s.len() {
            w.truncate(kept);
            n += self.table.append_next(w, &s[n..]);
            // Go checks the first rune of the whole input as the digit, not the rune just read.
            let digit = self.check_next_digit(kept, w, s);
            if n_digits == 0 && digit == Some(true) {
                continue;
            }
            kept = w.len();
            if digit.is_none() {
                break;
            }
            n_digits += 1;
            if n_digits >= MAX_DIGITS {
                break;
            }
        }
        w.truncate(kept);
        w[len_index] =
            Elem::make(n_digits, DEFAULT_SECONDARY, DEFAULT_TERTIARY, 0).unwrap_or_default();
        n
    }

    /// Whether the elements of `w` after the first `kept` are a digit, and whether it is zero; `None` when they
    /// are not a digit. `s` is the input whose first rune Go checks for category Nd.
    fn check_next_digit(&self, kept: usize, w: &[Elem], s: &[u8]) -> Option<bool> {
        if kept >= w.len() {
            return None;
        }
        let e = w[kept];
        if e < self.zero_special_lo || self.nine < e {
            // Not a number.
            return None;
        }
        let is_zero = if e < self.zero {
            if e > self.nine_special_hi || !is_digit(s) {
                // Not a number.
                return None;
            }
            e <= self.zero_special_hi
        } else {
            // This is the common case if we encounter a digit.
            e == self.zero
        };
        // Test the remaining added collation elements have a zero primary value.
        if w.len() - kept > 1 {
            if w[kept + 1..].iter().any(|e| e.primary() != 0) {
                return None;
            }
            // In some rare cases, collation elements encode runes outside Nd as a digit, such as the Ethiopic
            // digits (U+1369 - U+1371), which are in No.
            if !is_digit(s) {
                return None;
            }
        }
        Some(is_zero)
    }
}

fn is_digit(s: &[u8]) -> bool {
    if s.is_empty() {
        // Go decodes RuneError from an empty input.
        return false;
    }
    go_unicode::is_digit(first_rune(s).0 as u32)
}
