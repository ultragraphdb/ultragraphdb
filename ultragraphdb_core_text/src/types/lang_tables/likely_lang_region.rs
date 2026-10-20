/*
 * Copyright 2013 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from golang.org/x/text v0.41.0, internal/language/tables.go (the table row types). Changed for ultragraphdb:
 * translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

/// The most likely language and region of a script.
#[derive(Debug, Clone, Copy)]
pub(crate) struct LikelyLangRegion {
    pub lang: u16,
    pub region: u16,
}
