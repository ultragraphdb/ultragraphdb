/*
 * Copyright 2013 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from golang.org/x/text v0.41.0, internal/language/tables.go (the table row types). Changed for ultragraphdb:
 * translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

/// The most likely language and script of a region, or the index and length of its list when `flags` has
/// `IS_LIST`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct LikelyLangScript {
    pub lang: u16,
    pub script: u16,
    pub flags: u8,
}
