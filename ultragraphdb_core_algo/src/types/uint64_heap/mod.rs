/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, algo/heap.go, with the operations of Go's container/heap (Init, Push, Pop, Fix) it
 * is used through. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::types::elem::Elem;

/// A min-heap of [`Elem`]s ordered by value, the one Dgraph's merges use. Elements of equal value come out in the
/// order Go's container/heap gives them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Uint64Heap {
    elems: Vec<Elem>,
}

impl Uint64Heap {
    /// A heap of `elems`, ordered in O(n): Go's `heap.Init`.
    pub fn new(elems: Vec<Elem>) -> Self {
        let mut heap = Self { elems };
        let n = heap.elems.len();
        for i in (0..n / 2).rev() {
            heap.down(i, n);
        }
        heap
    }

    pub fn len(&self) -> usize {
        self.elems.len()
    }

    pub fn is_empty(&self) -> bool {
        self.elems.is_empty()
    }

    /// The element at `i`; the smallest at 0.
    pub fn get(&self, i: usize) -> Option<&Elem> {
        self.elems.get(i)
    }

    /// Replaces the element at `i`, which [`fix`](Self::fix) then moves to its place.
    ///
    /// # Panics
    ///
    /// When `i` is out of range.
    pub fn set(&mut self, i: usize, elem: Elem) {
        self.elems[i] = elem;
    }

    /// Adds an element: Go's `heap.Push`.
    pub fn push(&mut self, elem: Elem) {
        self.elems.push(elem);
        self.up(self.elems.len() - 1);
    }

    /// Removes and returns the smallest element: Go's `heap.Pop`.
    pub fn pop(&mut self) -> Option<Elem> {
        let n = self.elems.len().checked_sub(1)?;
        self.elems.swap(0, n);
        self.down(0, n);
        self.elems.pop()
    }

    /// Restores the order after the element at `i` changed: Go's `heap.Fix`.
    pub fn fix(&mut self, i: usize) {
        if !self.down(i, self.elems.len()) {
            self.up(i);
        }
    }

    fn less(&self, i: usize, j: usize) -> bool {
        self.elems[i].val() < self.elems[j].val()
    }

    fn up(&mut self, mut j: usize) {
        while j > 0 {
            let i = (j - 1) / 2; // parent
            if !self.less(j, i) {
                break;
            }
            self.elems.swap(i, j);
            j = i;
        }
    }

    fn down(&mut self, i0: usize, n: usize) -> bool {
        let mut i = i0;
        loop {
            let j1 = 2 * i + 1;
            if j1 >= n {
                break;
            }
            let mut j = j1; // left child
            let j2 = j1 + 1;
            if j2 < n && self.less(j2, j1) {
                j = j2; // = 2*i + 2  // right child
            }
            if !self.less(j, i) {
                break;
            }
            self.elems.swap(i, j);
            i = j;
        }
        i > i0
    }
}
