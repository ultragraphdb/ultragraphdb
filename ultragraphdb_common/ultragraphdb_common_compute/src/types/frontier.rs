/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::cmp::Ordering;

/// A merge's frontier: the partitions' streams holding a next row, by index, the lowest row's first, as a binary heap ordered
/// by a comparison the caller gives, since what it compares stays in the streams and nothing is copied per row. A pop and a push
/// cost the logarithm of the frontier, so a read over many partitions costs no more per row than a merge scan.
#[derive(Debug, Default)]
pub struct Frontier {
    heap: Vec<usize>,
}

impl Frontier {
    /// An empty frontier of room for `streams`.
    #[must_use]
    pub fn with_capacity(streams: usize) -> Self {
        Self {
            heap: Vec::with_capacity(streams),
        }
    }

    /// Adds `stream`.
    pub fn push(&mut self, stream: usize, order: impl Fn(usize, usize) -> Ordering) {
        self.heap.push(stream);
        let mut at = self.heap.len().saturating_sub(1);
        // Each turn moves the entry one level up, so the loop ends within the heap's depth (Ultra rule 2).
        while at > 0 {
            let parent = (at - 1) / 2;
            if !self.less(at, parent, &order) {
                return;
            }
            self.heap.swap(at, parent);
            at = parent;
        }
    }

    /// The stream of the lowest row, taken out.
    pub fn pop(&mut self, order: impl Fn(usize, usize) -> Ordering) -> Option<usize> {
        if self.heap.is_empty() {
            return None;
        }
        let top = self.heap.swap_remove(0);
        let mut at = 0;
        // Each turn moves the entry one level down, so the loop ends within the heap's depth (Ultra rule 2).
        loop {
            let (left, right) = (2 * at + 1, 2 * at + 2);
            let mut lowest = at;
            if left < self.heap.len() && self.less(left, lowest, &order) {
                lowest = left;
            }
            if right < self.heap.len() && self.less(right, lowest, &order) {
                lowest = right;
            }
            if lowest == at {
                return Some(top);
            }
            self.heap.swap(at, lowest);
            at = lowest;
        }
    }

    /// The streams the frontier holds, in no order.
    #[must_use]
    pub fn streams(&self) -> &[usize] {
        &self.heap
    }

    /// Whether the entry at `a` sorts below the one at `b`.
    fn less(&self, a: usize, b: usize, order: &impl Fn(usize, usize) -> Ordering) -> bool {
        match (self.heap.get(a), self.heap.get(b)) {
            (Some(a), Some(b)) => order(*a, *b) == Ordering::Less,
            _ => false,
        }
    }
}
