/*
 * Copyright 2013 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from golang.org/x/text v0.41.0, internal/language/tables.go (the table row types). Changed for ultragraphdb:
 * translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

/// The CLDR parent `lang-script-to_region` of the regional variants `lang-from_region` written in `max_script`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ParentRel {
    pub lang: u16,
    pub script: u16,
    pub max_script: u16,
    pub to_region: u16,
    pub from_region: &'static [u16],
}
