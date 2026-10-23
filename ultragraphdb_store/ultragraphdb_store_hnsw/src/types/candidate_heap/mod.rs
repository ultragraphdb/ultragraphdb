/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * Copyright 2009 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: Apache-2.0 AND BSD-3-Clause
 *
 * Ported from Dgraph v25.5.0, tok/hnsw/heap.go (minPersistentTupleHeap, maxPersistentTupleHeap, candidateHeap,
 * buildCandidateHeap), with Go 1.27's container/heap (Init, Push, Pop, Remove, up, down) it runs on. Changed for
 * ultragraphdb: translated to Rust; one heap whose order is chosen when it is built, where Dgraph wraps two types
 * behind an interface; Go's sift-up and sift-down exactly, so that `pop_last` drops the leaf Go drops.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::traits::hnsw_float::HnswFloat;
use crate::types::persistent_heap_element::PersistentHeapElement;

/// Dgraph's `candidateHeap`: the candidates a layer search has yet to expand, best first: a min-heap of distances
/// or a max-heap of similarities.
#[derive(Debug, Clone)]
pub struct CandidateHeap<T> {
    items: Vec<PersistentHeapElement<T>>,
    max: bool,
}

impl<T: HnswFloat> CandidateHeap<T> {
    /// Dgraph's `buildCandidateHeap`: a heap of `items`, a max-heap when `is_similarity_metric`, built in place in
    /// linear time as Go's `heap.Init`.
    pub fn build(items: Vec<PersistentHeapElement<T>>, is_similarity_metric: bool) -> Self {
        let mut heap = Self {
            items,
            max: is_similarity_metric,
        };
        let n = heap.items.len();
        for i in (0..n / 2).rev() {
            heap.down(i, n);
        }
        heap
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Go's `heap.Push`.
    pub fn push(&mut self, x: PersistentHeapElement<T>) {
        self.items.push(x);
        self.up(self.items.len() - 1);
    }

    /// Go's `heap.Pop`: the best candidate; `None` when there is none, where Go panics.
    pub fn pop(&mut self) -> Option<PersistentHeapElement<T>> {
        let n = self.items.len().checked_sub(1)?;
        self.items.swap(0, n);
        self.down(0, n);
        self.items.pop()
    }

    /// Dgraph's `PopLast`, Go's `heap.Remove` of the last element: drops the heap array's last leaf, which is not
    /// necessarily the worst candidate.
    pub fn pop_last(&mut self) {
        self.items.pop();
    }

    fn less(&self, i: usize, j: usize) -> bool {
        if self.max {
            self.items[i].value() > self.items[j].value()
        } else {
            self.items[i].value() < self.items[j].value()
        }
    }

    fn up(&mut self, mut j: usize) {
        while j > 0 {
            let i = (j - 1) / 2;
            if !self.less(j, i) {
                break;
            }
            self.items.swap(i, j);
            j = i;
        }
    }

    fn down(&mut self, i0: usize, n: usize) {
        let mut i = i0;
        loop {
            let j1 = 2 * i + 1;
            if j1 >= n {
                break;
            }
            let mut j = j1;
            let j2 = j1 + 1;
            if j2 < n && self.less(j2, j1) {
                j = j2;
            }
            if !self.less(j, i) {
                break;
            }
            self.items.swap(i, j);
            i = j;
        }
    }
}
