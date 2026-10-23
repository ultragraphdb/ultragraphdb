/*
 * Copyright 2012 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from golang.org/x/text v0.41.0, internal/colltab/contract.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::tables::collate_tables::MAIN_CT_ENTRIES;

const FINAL: u8 = 0;
const NO_INDEX: u8 = 0xFF;

/// Matches a contraction trie to an input sequence. A contraction may match a non-contiguous sequence: for
/// `<a, combining_ring>`, `<a, combining_cedilla, combining_ring>` matches, as the cedilla does not block the
/// ring. The scanner does not skip non-blocking non-starters itself; it keeps the state of the last match and
/// leaves it to the caller to continue the match at the right points.
#[derive(Debug, Clone)]
pub(crate) struct CtScanner<'a> {
    /// The trie states, as an offset into the contraction trie set.
    states: usize,
    /// The input; `s_end` bounds the part the scanner reads.
    s: &'a [u8],
    pub s_end: usize,
    n: usize,
    index: usize,
    pub pindex: usize,
    pub done: bool,
}

impl<'a> CtScanner<'a> {
    pub(crate) fn new(index: usize, n: usize, b: &'a [u8]) -> CtScanner<'a> {
        CtScanner {
            states: index,
            s: b,
            s_end: b.len(),
            n,
            index: 0,
            pindex: 0,
            done: false,
        }
    }

    /// The offset into the contraction elements and the bytes consumed so far; both zero when no suffix
    /// matched.
    pub(crate) fn result(&self) -> (usize, usize) {
        (self.index, self.pindex)
    }

    /// Matches the longest suffix at position `p` and returns the number of bytes consumed.
    pub(crate) fn scan(&mut self, mut p: usize) -> usize {
        let mut pr = p; // the p at the rune start
        let str = &self.s[..self.s_end];
        let (mut states, mut n) = (self.states, self.n);
        let mut i = 0;
        while i < n && p < str.len() {
            let [l, h, nn, ii] = MAIN_CT_ENTRIES[states + i];
            let c = str[p];
            if c >= l {
                if l == c {
                    p += 1;
                    if ii != NO_INDEX {
                        self.index = usize::from(ii);
                        self.pindex = p;
                    }
                    if nn != FINAL {
                        i = 0;
                        states += usize::from(h) + n;
                        n = usize::from(nn);
                        if p >= str.len() || is_rune_start(str[p]) {
                            self.states = states;
                            self.n = n;
                            pr = p;
                        }
                    } else {
                        self.done = true;
                        return p;
                    }
                    continue;
                } else if nn == FINAL && c <= h {
                    p += 1;
                    self.done = true;
                    self.index = usize::from(c - l) + usize::from(ii);
                    self.pindex = p;
                    return p;
                }
            }
            i += 1;
        }
        pr
    }
}

fn is_rune_start(b: u8) -> bool {
    b & 0xC0 != 0x80
}
