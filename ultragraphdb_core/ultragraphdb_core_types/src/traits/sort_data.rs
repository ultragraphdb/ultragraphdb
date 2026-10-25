/*
 * Copyright 2009 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from Go 1.27, src/sort/sort.go (Interface). Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

/// Go's `sort.Interface`: what Go's pdqsort and Dgraph's quickselect move and compare.
pub(crate) trait SortData {
    fn len(&self) -> usize;
    fn less(&self, i: usize, j: usize) -> bool;
    fn swap(&mut self, i: usize, j: usize);
}
