/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

/// A key held in a run's arena, by offset and length.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub(crate) struct Span {
    pub at: u32,
    pub len: u32,
}

impl Span {
    /// The key's bytes in `arena`; empty when the span lies outside it, which a run never builds.
    pub fn of(self, arena: &[u8]) -> &[u8] {
        let start = self.at as usize;
        arena.get(start..start + self.len as usize).unwrap_or(&[])
    }
}
