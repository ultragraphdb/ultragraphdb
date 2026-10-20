/*
 * Copyright 2013 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from golang.org/x/text v0.41.0, collate/index.go. Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

/// Where a locale's tailoring starts in the main trie, in blocks of 64 entries.
#[derive(Debug, Clone, Copy)]
pub(crate) struct TableIndex {
    pub lookup_offset: u32,
    pub values_offset: u32,
}
