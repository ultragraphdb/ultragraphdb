/*
 * Copyright 2009 The Go Authors. All rights reserved.
 * SPDX-License-Identifier: BSD-3-Clause
 *
 * Ported from Go 1.27, src/strings/strings.go (ToLower). Changed for ultragraphdb: translated to Rust.
 * Copyright (c) 2026. Marvin Hansen.
 */

//! Go's `strings` as the port uses it.

use crate::utils::go_unicode;

/// Go's `strings.ToLower`: each rune by `unicode.ToLower`, its simple lowercase mapping, which takes `İ` (U+0130)
/// to `i` where Rust's full mapping gives `i̇`, and a final `Σ` to `σ`.
pub fn to_lower(s: &str) -> String {
    if s.is_ascii() {
        return s.to_ascii_lowercase();
    }
    s.chars()
        .map(|c| char::from_u32(go_unicode::to_lower(c)).unwrap_or(c))
        .collect()
}
