/*
 * Copyright 2010 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from Go 1.27, sort/search.go (Search). Changed for ultragraphdb: translated to Rust; the predicate may
 * fail, and its error ends the search.
 * Copyright (c) 2026. Marvin Hansen.
 */

/// Go's `sort.Search`: the smallest index in `[0, n)` at which `f` holds, `n` when it holds nowhere, by Go's own
/// bisection, so that a predicate that is not monotone (a NaN score) picks the index Go picks.
pub(crate) fn search<E>(n: usize, mut f: impl FnMut(usize) -> Result<bool, E>) -> Result<usize, E> {
    let (mut i, mut j) = (0usize, n);
    while i < j {
        let h = i + (j - i) / 2;
        if !f(h)? {
            i = h + 1;
        } else {
            j = h;
        }
    }
    Ok(i)
}
