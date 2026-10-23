/*
 * Copyright 2012 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from golang.org/x/text v0.41.0, collate/collate.go (Key, KeyFromString and the key encoding). Changed for
 * ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use super::Collator;
use crate::types::elem::{Elem, IGNORE, MAX_QUATERNARY, QUATERNARY, SECONDARY, TERTIARY};
use crate::types::iter::Iter;
use crate::types::options::AlternateHandling;

impl Collator {
    /// The collation key of `s`, as Go's `KeyFromString` writes it into a reset `collate.Buffer`: keys compare
    /// byte-wise in the collation order.
    pub fn key(&self, s: &str) -> Vec<u8> {
        self.key_bytes(s.as_bytes())
    }

    /// The collation key of `s`, which need not be valid UTF-8, as Go's `Key` (and `KeyFromString` over a Go string
    /// holding the same bytes) writes it into a reset `collate.Buffer`. Go weighs invalid UTF-8 as U+FFFD's implicit
    /// weight (`85 FF FD`), one weight per trie lookup: a sequence the trie reads to its last byte (an overlong
    /// encoding, a surrogate, a code point past U+10FFFF) or up to a byte that does not continue it weighs once, any
    /// other invalid byte (a lone continuation byte, `C0`-`FF` where no sequence fits, a sequence cut short by the
    /// end of the input) weighs once on its own.
    pub fn key_bytes(&self, s: &[u8]) -> Vec<u8> {
        // See https://www.unicode.org/reports/tr10/#Main_Algorithm for more details.
        let mut it = Iter::new(&self.options.t, s);
        while it.next() {}
        let mut w = it.elems;
        process_weights(self.options.alternate, self.options.t.top(), &mut w);
        self.key_from_elems(&w)
    }

    /// Converts the weights to a compact sequence of bytes.
    fn key_from_elems(&self, ws: &[Elem]) -> Vec<u8> {
        let o = &self.options;
        let mut key = Vec::new();
        for v in ws {
            let w = v.primary();
            if w > 0 {
                append_primary(&mut key, w);
            }
        }
        if !o.ignore[SECONDARY] {
            key.extend_from_slice(&[0, 0]);
            let mut push = |w: i32| {
                if w > 0 {
                    key.push((w >> 8) as u8);
                    key.push(w as u8);
                }
            };
            if !o.backwards {
                ws.iter().for_each(|v| push(v.secondary()));
            } else {
                ws.iter().rev().for_each(|v| push(v.secondary()));
            }
        } else if o.case_level {
            key.extend_from_slice(&[0, 0]);
        }
        if !o.ignore[TERTIARY] || o.case_level {
            key.extend_from_slice(&[0, 0]);
            for v in ws {
                let w = v.tertiary();
                if w > 0 {
                    key.push(w);
                }
            }
            // Derive the quaternary weights from the options and other levels. MaxQuaternary is 0xFF: the first
            // byte of a primary weight is always smaller, so this single byte compares correctly.
            if !o.ignore[QUATERNARY] && o.alternate >= AlternateHandling::Shifted {
                if o.alternate == AlternateHandling::ShiftTrimmed {
                    let mut last_non_ffff = key.len();
                    key.push(0);
                    for v in ws {
                        let w = v.quaternary();
                        if w == MAX_QUATERNARY {
                            key.push(0xFF);
                        } else if w > 0 {
                            append_primary(&mut key, w);
                            last_non_ffff = key.len();
                        }
                    }
                    key.truncate(last_non_ffff);
                } else {
                    key.push(0);
                    for v in ws {
                        let w = v.quaternary();
                        if w == MAX_QUATERNARY {
                            key.push(0xFF);
                        } else if w > 0 {
                            append_primary(&mut key, w);
                        }
                    }
                }
            }
        }
        key
    }
}

/// Appends a primary weight in a variable-length encoding of up to 23 bits.
fn append_primary(key: &mut Vec<u8>, p: i32) {
    if p <= 0x7FFF {
        key.extend_from_slice(&[(p >> 8) as u8, p as u8]);
    } else {
        key.extend_from_slice(&[(p >> 16) as u8 | 0x80, (p >> 8) as u8, p as u8]);
    }
}

/// Applies the handling of variables to the weights.
pub(crate) fn process_weights(vw: AlternateHandling, top: u32, wa: &mut [Elem]) {
    let mut ignore = false;
    let vtop = top as i32;
    match vw {
        AlternateHandling::Shifted | AlternateHandling::ShiftTrimmed => {
            for e in wa.iter_mut() {
                let p = e.primary();
                if p <= vtop && p != 0 {
                    *e = Elem::make_quaternary(p);
                    ignore = true;
                } else if p == 0 {
                    if ignore {
                        *e = IGNORE;
                    }
                } else {
                    ignore = false;
                }
            }
        }
        AlternateHandling::Blanked => {
            for e in wa.iter_mut() {
                let p = e.primary();
                if p <= vtop && (ignore || p != 0) {
                    *e = IGNORE;
                    ignore = true;
                } else {
                    ignore = false;
                }
            }
        }
        AlternateHandling::NonIgnorable => {}
    }
}
