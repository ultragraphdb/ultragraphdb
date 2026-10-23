/*
 * Copyright 2012 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from golang.org/x/text v0.41.0, collate/collate.go (Compare and CompareString). Changed for ultragraphdb:
 * translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use std::cmp::Ordering;

use super::Collator;
use crate::types::elem::{IDENTITY, QUATERNARY, SECONDARY, TERTIARY};
use crate::types::iter::Iter;
use crate::types::options::AlternateHandling;

impl Collator {
    /// Compares two strings in the collation order, as Go's `CompareString`. Strings that collate equal compare
    /// equal unless the identical level is enabled (`-u-ks-identic`).
    pub fn compare(&self, a: &str, b: &str) -> Ordering {
        self.compare_bytes(a.as_bytes(), b.as_bytes())
    }

    /// Compares two byte strings, which need not be valid UTF-8, in the collation order, as Go's `Compare` (and
    /// `CompareString` over Go strings holding the same bytes); invalid UTF-8 weighs as in
    /// [`Collator::key_bytes`]. Strings that collate equal compare equal unless the identical level is enabled,
    /// which compares their bytes.
    pub fn compare_bytes(&self, a: &[u8], b: &[u8]) -> Ordering {
        let mut ia = Iter::new(&self.options.t, a);
        let mut ib = Iter::new(&self.options.t, b);
        let res = self.compare_iters(&mut ia, &mut ib);
        if res != Ordering::Equal {
            return res;
        }
        if !self.options.ignore[IDENTITY] {
            return a.cmp(b);
        }
        Ordering::Equal
    }

    fn compare_iters<'x>(&self, ia: &mut Iter<'x>, ib: &mut Iter<'x>) -> Ordering {
        let o = &self.options;
        // Process primary level. Go skips it for shifted variables (not implemented in Go).
        if o.alternate != AlternateHandling::Shifted {
            let res = compare_level(Iter::next_primary, ia, ib);
            if res != Ordering::Equal {
                return res;
            }
        }
        if !o.ignore[SECONDARY] {
            let f = if o.backwards {
                Iter::prev_secondary
            } else {
                Iter::next_secondary
            };
            let res = compare_level(f, ia, ib);
            if res != Ordering::Equal {
                return res;
            }
        }
        if !o.ignore[TERTIARY] || o.case_level {
            let res = compare_level(Iter::next_tertiary, ia, ib);
            if res != Ordering::Equal {
                return res;
            }
            if !o.ignore[QUATERNARY] {
                let res = compare_level(Iter::next_quaternary, ia, ib);
                if res != Ordering::Equal {
                    return res;
                }
            }
        }
        Ordering::Equal
    }
}

fn compare_level<'x>(f: fn(&mut Iter<'x>) -> i32, a: &mut Iter<'x>, b: &mut Iter<'x>) -> Ordering {
    a.reset_level();
    b.reset_level();
    loop {
        let va = f(a);
        let vb = f(b);
        if va != vb {
            return va.cmp(&vb);
        }
        if va == 0 {
            return Ordering::Equal;
        }
    }
}
