/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// Go's `strings.ToLower`: each rune by its simple lowercase mapping, which takes `İ` (U+0130) to `i` where Rust's
/// full mapping gives `i̇`.
pub(crate) fn go_to_lower(s: &str) -> String {
    if s.is_ascii() {
        return s.to_ascii_lowercase();
    }
    s.chars()
        .flat_map(|c| {
            if c == '\u{130}' {
                'i'.to_lowercase()
            } else {
                c.to_lowercase()
            }
        })
        .collect()
}
