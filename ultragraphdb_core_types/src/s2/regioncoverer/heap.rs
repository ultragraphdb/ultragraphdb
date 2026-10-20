/*
 * Copyright 2009 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from Go 1.26.4, src/container/heap/heap.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

//! Go's `container/heap` algorithms over a vector, so that candidates of equal priority leave
//! the queue in the order Go's heap gives them.

/// Pushes the element x onto the heap. `less(a, b)` orders a before b.
pub(crate) fn push<T>(h: &mut Vec<T>, x: T, less: impl Fn(&T, &T) -> bool) {
    h.push(x);
    let n = h.len() - 1;
    up(h, n, &less);
}

/// Removes and returns the minimum element (according to less) from the heap.
pub(crate) fn pop<T>(h: &mut Vec<T>, less: impl Fn(&T, &T) -> bool) -> Option<T> {
    if h.is_empty() {
        return None;
    }
    let n = h.len() - 1;
    h.swap(0, n);
    down(h, 0, n, &less);
    h.pop()
}

fn up<T>(h: &mut [T], j: usize, less: &impl Fn(&T, &T) -> bool) {
    let mut j = j;
    while j > 0 {
        let i = (j - 1) / 2; // parent
        if !less(&h[j], &h[i]) {
            break;
        }
        h.swap(i, j);
        j = i;
    }
}

fn down<T>(h: &mut [T], i0: usize, n: usize, less: &impl Fn(&T, &T) -> bool) -> bool {
    let mut i = i0;
    loop {
        let j1 = 2 * i + 1;
        if j1 >= n {
            break;
        }
        let mut j = j1; // left child
        let j2 = j1 + 1;
        if j2 < n && less(&h[j2], &h[j1]) {
            j = j2; // = 2*i + 2  // right child
        }
        if !less(&h[j], &h[i]) {
            break;
        }
        h.swap(i, j);
        i = j;
    }
    i > i0
}
