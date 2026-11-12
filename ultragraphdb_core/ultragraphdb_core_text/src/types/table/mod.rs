/*
 * Copyright 2012 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from golang.org/x/text v0.41.0, internal/colltab/table.go, internal/colltab/trie.go and collate/index.go.
 * Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

mod table_contraction;

use crate::tables::collate_tables::{MAIN_EXPAND_ELEM, MAIN_LOOKUP, MAIN_VALUES, VAR_TOP};
use crate::types::elem::{CeType, Elem, MAX_TERTIARY, implicit_primary};
use crate::types::table_index::TableIndex;
use crate::utils::norm::{first_rune, hangul_nfd, nfkd_decomposition};

const BLOCK_SIZE: usize = 64;

/// The collation table of one locale: the main trie entered at the locale's tailoring.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Table {
    /// Index for the first byte (0xC0-0xFF).
    index0: usize,
    /// Values for the first byte (0x00-0x7F).
    values0: usize,
}

const TX: u8 = 0x80;
const T2: u8 = 0xC0;
const T3: u8 = 0xE0;
const T4: u8 = 0xF0;
const T5: u8 = 0xF8;

const HANGUL_SIZE: usize = 3;
const FIRST_HANGUL: u32 = 0xAC00;
const LAST_HANGUL: u32 = 0xD7A3;

impl Table {
    /// `getTable`: the table of a locale.
    pub(crate) fn new(t: TableIndex) -> Table {
        Table {
            index0: BLOCK_SIZE * t.lookup_offset as usize,
            values0: BLOCK_SIZE * t.values_offset as usize,
        }
    }

    /// The highest variable primary weight.
    pub(crate) fn top(&self) -> u32 {
        VAR_TOP
    }

    fn lookup_value(n: u16, b: u8) -> Elem {
        value((usize::from(n) << 6) + usize::from(b))
    }

    /// The trie value of the first UTF-8 encoding in `s` and its width in bytes; the width is zero when `s`
    /// holds too few bytes to complete the encoding. `s` is not empty.
    pub(crate) fn lookup(&self, s: &[u8]) -> (Elem, usize) {
        let Some(&c0) = s.first() else {
            return (Elem(0), 0);
        };
        let index0 = |c: u8| lookup_block(self.index0 + usize::from(c));
        if c0 < TX {
            return (value(self.values0 + usize::from(c0)), 1);
        }
        if c0 < T2 {
            return (Elem(0), 1);
        }
        let continuation = |c: u8| (TX..T2).contains(&c);
        if c0 < T3 {
            let [_, c1, ..] = *s else {
                return (Elem(0), 0);
            };
            let i = index0(c0);
            if !continuation(c1) {
                return (Elem(0), 1);
            }
            return (Table::lookup_value(i, c1), 2);
        }
        if c0 < T4 {
            let [_, c1, c2, ..] = *s else {
                return (Elem(0), 0);
            };
            let i = index0(c0);
            if !continuation(c1) {
                return (Elem(0), 1);
            }
            let i = lookup_block((usize::from(i) << 6) + usize::from(c1));
            if !continuation(c2) {
                return (Elem(0), 2);
            }
            return (Table::lookup_value(i, c2), 3);
        }
        if c0 < T5 {
            let [_, c1, c2, c3, ..] = *s else {
                return (Elem(0), 0);
            };
            let i = index0(c0);
            if !continuation(c1) {
                return (Elem(0), 1);
            }
            let i = lookup_block((usize::from(i) << 6) + usize::from(c1));
            if !continuation(c2) {
                return (Elem(0), 2);
            }
            let i = lookup_block((usize::from(i) << 6) + usize::from(c2));
            if !continuation(c3) {
                return (Elem(0), 3);
            }
            return (Table::lookup_value(i, c3), 4);
        }
        // Illegal rune
        (Elem(0), 1)
    }

    /// Appends the elements of the next rune or contraction in `s` to `w` and returns the number of bytes
    /// consumed. A contraction matched to a discontiguous sequence also appends the elements of the runes in
    /// between.
    pub(crate) fn append_next(&self, w: &mut Vec<Elem>, s: &[u8]) -> usize {
        let (mut ce, sz) = self.lookup(s);
        match ce.ctype() {
            CeType::Normal => {
                if ce.0 == 0 {
                    let (r, _) = first_rune(s);
                    let r = r as u32;
                    if (FIRST_HANGUL..=LAST_HANGUL).contains(&r) {
                        let n = sz;
                        let decomposed = hangul_nfd(char::from_u32(r).unwrap_or_default());
                        debug_assert!(s.len() >= HANGUL_SIZE);
                        let mut b = decomposed.as_slice();
                        while !b.is_empty() {
                            let (hangul_ce, hangul_sz) = self.lookup(b);
                            w.push(hangul_ce);
                            b = b.get(hangul_sz..).unwrap_or_default();
                        }
                        return n;
                    }
                    ce = Elem::make_implicit(implicit_primary(r));
                }
                w.push(ce);
                sz
            }
            CeType::ExpansionIndex => {
                self.append_expansion(w, ce);
                sz
            }
            CeType::ContractionIndex => {
                let n = self.match_contraction(w, ce, s.get(sz..).unwrap_or_default());
                sz + n
            }
            CeType::Decompose => {
                // Decompose using NFKD and replace tertiary weights.
                let (t1, t2) = ce.split_decompose();
                let first = w.len();
                let (r, _) = first_rune(s);
                let nfkd = nfkd_decomposition(r).unwrap_or_default();
                let mut b = nfkd.as_bytes();
                // The recursion is one level deep: a rune of an NFKD decomposition decomposes to itself, so the table
                // gives it no `Decompose` element. Each turn consumes at least one byte of the decomposition.
                while !b.is_empty() {
                    let p = self.append_next(w, b);
                    b = b.get(p..).unwrap_or_default();
                }
                // The first rune's elements take t1, the second's t2 and every later one the largest tertiary.
                let tertiaries = [t1, t2].into_iter().chain(std::iter::repeat(MAX_TERTIARY));
                for (e, t) in w.iter_mut().skip(first).zip(tertiaries) {
                    *e = e.update_tertiary(t);
                }
                sz
            }
        }
    }

    pub(crate) fn append_expansion(&self, w: &mut Vec<Elem>, ce: Elem) {
        let i = ce.split_expand_index();
        // The expansion's length, then its elements.
        let Some((&n, elems)) = MAIN_EXPAND_ELEM.get(i..).and_then(<[u32]>::split_first) else {
            return;
        };
        w.extend(elems.iter().take(n as usize).map(|&e| Elem(e)));
    }
}

/// The main trie's value at `i`.
fn value(i: usize) -> Elem {
    Elem(MAIN_VALUES.get(i).copied().unwrap_or_default())
}

/// The main trie's block at `i`.
fn lookup_block(i: usize) -> u16 {
    MAIN_LOOKUP.get(i).copied().unwrap_or_default()
}
