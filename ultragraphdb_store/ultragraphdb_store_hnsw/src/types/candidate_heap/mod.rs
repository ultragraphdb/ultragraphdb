/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * After Dgraph v25.5.0, tok/hnsw/heap.go (candidateHeap, buildCandidateHeap). Changed for ultragraphdb: a binary
 * heap written for it, one type whose order is chosen when it is built.
 * Copyright (c) 2026. Marvin Hansen.
 */

use crate::traits::hnsw_float::HnswFloat;
use crate::types::persistent_heap_element::PersistentHeapElement;

/// The candidates a layer search has yet to expand, best first: the least distance, or the greatest similarity. A
/// binary heap in one vector, each candidate before both of its children.
#[derive(Debug, Clone)]
pub struct CandidateHeap<T> {
    items: Vec<PersistentHeapElement<T>>,
    max: bool,
}

impl<T: HnswFloat> CandidateHeap<T> {
    /// A heap of `items`, the greatest first when `is_similarity_metric` and the least first otherwise, ordered in
    /// place in linear time.
    pub fn build(items: Vec<PersistentHeapElement<T>>, is_similarity_metric: bool) -> Self {
        let mut heap = Self {
            items,
            max: is_similarity_metric,
        };
        // Every item of the first half has a child; the rest are leaves.
        (0..heap.items.len() / 2)
            .rev()
            .for_each(|at| heap.sift_down(at));
        heap
    }

    /// The number of candidates.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Whether the heap holds no candidate.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Adds a candidate.
    pub fn push(&mut self, x: PersistentHeapElement<T>) {
        let at = self.items.len();
        self.items.push(x);
        self.sift_up(at);
    }

    /// Takes the best candidate out; `None` when there is none.
    pub fn pop(&mut self) -> Option<PersistentHeapElement<T>> {
        if self.items.is_empty() {
            return None;
        }
        // The last leaf takes the best's place and sinks to its own.
        let best = self.items.swap_remove(0);
        self.sift_down(0);
        Some(best)
    }

    /// Drops the heap's last leaf in constant time, which bounds the heap; a leaf is no better than its ancestors,
    /// and not necessarily the worst candidate.
    pub fn pop_last(&mut self) {
        let _last: Option<PersistentHeapElement<T>> = self.items.pop();
    }

    /// Whether the candidate at `a` goes before the one at `b`; false when either is past the heap.
    fn before(&self, a: usize, b: usize) -> bool {
        match (self.items.get(a), self.items.get(b)) {
            (Some(a), Some(b)) if self.max => a.value() > b.value(),
            (Some(a), Some(b)) => a.value() < b.value(),
            _ => false,
        }
    }

    /// Moves the candidate at `at` towards the root while it goes before its parent.
    fn sift_up(&mut self, mut at: usize) {
        // Ends: `at` at least halves each turn.
        while let Some(parent) = at.checked_sub(1).map(|below| below / 2) {
            if !self.before(at, parent) {
                break;
            }
            self.items.swap(at, parent);
            at = parent;
        }
    }

    /// Moves the candidate at `at` towards the leaves while a child goes before it.
    fn sift_down(&mut self, mut at: usize) {
        // Ends: `at` at least doubles each turn, and `before` is false past the heap.
        loop {
            let left = at.saturating_mul(2).saturating_add(1);
            let right = left.saturating_add(1);
            let child = if self.before(right, left) {
                right
            } else {
                left
            };
            if !self.before(child, at) {
                break;
            }
            self.items.swap(at, child);
            at = child;
        }
    }
}
