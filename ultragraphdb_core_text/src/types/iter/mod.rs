/*
 * Copyright 2015 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from golang.org/x/text v0.41.0, internal/colltab/iter.go and collate/collate.go (type iter). Changed for
 * ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::types::elem::Elem;
use crate::types::weighter::Weighter;

const MAX_COMBINING_CHARACTERS: usize = 30;

/// Converts the input incrementally to collation elements, keeping them in normalized order: in the order they
/// would have if the input were normalized first.
#[derive(Debug)]
pub(crate) struct Iter<'a> {
    weighter: &'a Weighter,
    pub elems: Vec<Elem>,
    /// The number of elements that are not reordered by later iterations.
    pub n: usize,
    bytes: &'a [u8],
    /// End position in the input of the elements up to `n`.
    p_end: usize,
    /// Start of the next call to `append_next`; `p_end <= p_next`.
    p_next: usize,
    /// The position of the next element a level iteration reads.
    pce: usize,
}

impl<'a> Iter<'a> {
    pub(crate) fn new(weighter: &'a Weighter, input: &'a [u8]) -> Iter<'a> {
        Iter {
            weighter,
            elems: Vec::with_capacity(512),
            n: 0,
            bytes: input,
            p_end: 0,
            p_next: 0,
            pce: 0,
        }
    }

    fn done(&self) -> bool {
        self.p_next >= self.bytes.len()
    }

    fn append_next(&mut self) -> bool {
        if self.done() {
            return false;
        }
        let mut sz = self
            .weighter
            .append_next(&mut self.elems, &self.bytes[self.p_next..]);
        if sz == 0 {
            sz = 1;
        }
        self.p_next += sz;
        true
    }

    /// Appends elements: either starters or modifiers. Mostly an element with a primary weight has a combining
    /// class of zero; the combining classes also detect input that is not normalized, and reorder it.
    pub(crate) fn next(&mut self) -> bool {
        if self.n == self.elems.len() && !self.append_next() {
            return false;
        }

        // Check if the current segment starts with a starter.
        let mut prev_ccc = self.elems[self.elems.len() - 1].ccc();
        if prev_ccc == 0 {
            self.n = self.elems.len();
            self.p_end = self.p_next;
            return true;
        } else if self.elems[self.n].ccc() == 0 {
            // Set n to cover only the part of elems for which prev_ccc == 0 and use the rest for the next call.
            self.n += 1;
            while self.n < self.elems.len() && self.elems[self.n].ccc() == 0 {
                self.n += 1;
            }
            self.p_end = self.p_next;
            return true;
        }

        // The current (partial) segment starts with modifiers. Collect all successive modifiers to ensure that
        // they are normalized.
        loop {
            let p = self.elems.len();
            self.p_end = self.p_next;
            if !self.append_next() {
                break;
            }
            let ccc = self.elems[p].ccc();
            if ccc == 0 || self.elems.len() - self.n > MAX_COMBINING_CHARACTERS {
                // Leave the starter for the next iteration, so that no sequence of elements crosses two
                // segments.
                self.n = p;
                return true;
            } else if ccc < prev_ccc {
                self.do_norm(p, ccc); // should be rare, never occurs for NFD and FCC.
            } else {
                prev_ccc = ccc;
            }
        }

        let done = self.elems.len() != self.n;
        self.n = self.elems.len();
        done
    }

    /// Reorders the elements: blocks added by `append_next` either start and end with the same combining class
    /// or start with zero, so the whole block has a single insertion point.
    fn do_norm(&mut self, mut p: usize, ccc: u8) {
        let k = p;
        p -= 1;
        while p > self.n && ccc < self.elems[p - 1].ccc() {
            p -= 1;
        }
        self.elems[p..].rotate_left(k - p);
    }

    pub(crate) fn next_primary(&mut self) -> i32 {
        loop {
            while self.pce < self.n {
                let v = self.elems[self.pce].primary();
                self.pce += 1;
                if v != 0 {
                    return v;
                }
            }
            if !self.next() {
                return 0;
            }
        }
    }

    pub(crate) fn next_secondary(&mut self) -> i32 {
        while self.pce < self.elems.len() {
            let v = self.elems[self.pce].secondary();
            self.pce += 1;
            if v != 0 {
                return v;
            }
        }
        0
    }

    pub(crate) fn prev_secondary(&mut self) -> i32 {
        while self.pce < self.elems.len() {
            let v = self.elems[self.elems.len() - self.pce - 1].secondary();
            self.pce += 1;
            if v != 0 {
                return v;
            }
        }
        0
    }

    pub(crate) fn next_tertiary(&mut self) -> i32 {
        while self.pce < self.elems.len() {
            let v = self.elems[self.pce].tertiary();
            self.pce += 1;
            if v != 0 {
                return i32::from(v);
            }
        }
        0
    }

    pub(crate) fn next_quaternary(&mut self) -> i32 {
        while self.pce < self.elems.len() {
            let v = self.elems[self.pce].quaternary();
            self.pce += 1;
            if v != 0 {
                return v;
            }
        }
        0
    }

    /// Starts a level iteration from the first element.
    pub(crate) fn reset_level(&mut self) {
        self.pce = 0;
    }
}
