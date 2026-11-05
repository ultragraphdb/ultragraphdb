/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::num::NonZeroUsize;

/// How far a walk goes on its own ([`crate::walk_lists`]): at most `rows` rows, and at most `idle` keys giving nothing before
/// it hands back, so a range of emptied lists takes many short walks and no long one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WalkBounds {
    rows: Option<NonZeroUsize>,
    idle: Option<NonZeroUsize>,
}

impl WalkBounds {
    /// A walk of at most `rows` rows, or the whole range with `None`, however many keys give nothing.
    pub fn rows(rows: Option<NonZeroUsize>) -> Self {
        Self { rows, idle: None }
    }

    /// The walk handing back after `idle` keys that gave nothing, when given.
    pub fn with_idle(mut self, idle: Option<NonZeroUsize>) -> Self {
        self.idle = idle;
        self
    }

    /// The most rows it gives.
    pub fn most_rows(&self) -> Option<NonZeroUsize> {
        self.rows
    }

    /// The most keys giving nothing it reads.
    pub fn most_idle(&self) -> Option<NonZeroUsize> {
        self.idle
    }
}
