/*
 * SPDX-FileCopyrightText: © 2017-2026 Istari Digital, Inc.
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ported from Dgraph v25.5.0, algo/heap.go. Changed for ultragraphdb: translated to Rust; a merge of packed lists
 * keeps each list's decoder beside the heap, indexed by the element's list, rather than in the element.
 * Copyright (c) 2026. Marvin Hansen.
 */

/// An element of a [`Uint64Heap`](crate::Uint64Heap): a value and the list it comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Elem {
    val: u64,
    list_idx: usize,
}

impl Elem {
    pub fn new(val: u64, list_idx: usize) -> Self {
        Self { val, list_idx }
    }

    pub fn val(&self) -> u64 {
        self.val
    }

    pub fn list_idx(&self) -> usize {
        self.list_idx
    }
}
