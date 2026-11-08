/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use std::sync::Arc;

use ultragraphdb_store_posting::List;

use crate::types::page_end::PageEnd;

/// What a page of the read kernel did ([`crate::QuadStore::page`]): how it ended and the rows it completed. A key paused
/// inside is a row of the page that finishes it, so a limit counts it once across pages.
#[derive(Debug, Clone, PartialEq)]
pub struct Page {
    end: PageEnd,
    rows: usize,
    held: Option<Arc<List>>,
}

impl Page {
    /// The page ending with `end`, having completed `rows` rows.
    #[must_use]
    pub fn new(end: PageEnd, rows: usize) -> Self {
        Self {
            end,
            rows,
            held: None,
        }
    }

    /// The page holding `held`, the list of the key it paused inside.
    #[must_use]
    pub fn with_held(mut self, held: Option<Arc<List>>) -> Self {
        self.held = held;
        self
    }

    /// How the page ended.
    #[must_use]
    pub fn end(&self) -> &PageEnd {
        &self.end
    }

    /// How the page ended, taken.
    #[must_use]
    pub fn into_end(self) -> PageEnd {
        self.end
    }

    /// How the page ended, and the list of the key it paused inside, for the next page to read on ([`crate::KernelScan::holding`]).
    #[must_use]
    pub fn into_parts(self) -> (PageEnd, Option<Arc<List>>) {
        (self.end, self.held)
    }

    /// The rows the page completed.
    #[must_use]
    pub fn rows(&self) -> usize {
        self.rows
    }
}
