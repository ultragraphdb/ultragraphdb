/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// A key's bytes in a read set's arena: their offset and length.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Span {
    pub at: u32,
    pub len: u32,
}

impl Span {
    /// The bytes of `arena` the span names; empty when it runs past, which a read set's own spans never do.
    pub fn of(self, arena: &[u8]) -> &[u8] {
        let start = self.at as usize;
        arena
            .get(start..start + self.len as usize)
            .unwrap_or_default()
    }
}
