/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// The heap allocations the process has made since it started: a counting global allocator's in `alloc_bench`, counts
/// moved by hand in a test.
pub trait AllocCount {
    /// The allocations, the reallocations and the bytes they asked for, each counted from the process's start.
    fn counted(&self) -> (u64, u64, u64);
}
