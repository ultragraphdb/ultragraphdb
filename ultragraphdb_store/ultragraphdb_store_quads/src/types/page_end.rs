/*
 * SPDX-License-Identifier: Apache-2.0
 * Copyright (c) 2026. Marvin Hansen. All Rights Reserved.
 */

use ultragraphdb_store_posting::PostingIdentity;

/// How a page of the read kernel ended ([`crate::Page`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PageEnd {
    /// The range holds no more keys.
    Exhausted,
    /// The page gave the scan's limit of rows, and more keys follow.
    Stopped {
        /// The key of the last row.
        last: Vec<u8>,
    },
    /// The sink stopped the page.
    Paused {
        /// The last key the sink took anything of; `None` when it took nothing, so nothing was read.
        last: Option<Vec<u8>>,
        /// The key to resume from: the key paused inside, or the last key followed by a zero byte, where the previous span
        /// ended, when paused between keys.
        from: Vec<u8>,
        /// The posting to resume after when paused inside a key.
        after: Option<PostingIdentity>,
    },
    /// The page read its bound of keys giving nothing ([`crate::KernelScan::yielding`]) and hands back, so what it took, which may
    /// be nothing, goes out and others get a turn.
    Yielded {
        /// The key to resume from, which what the page covered reaches.
        from: Vec<u8>,
    },
}

impl PageEnd {
    /// The key a next scan of the range begins at; `None` when the range is exhausted.
    pub fn resume_from(self) -> Option<Vec<u8>> {
        match self {
            Self::Exhausted => None,
            Self::Stopped { mut last } => {
                last.push(0);
                Some(last)
            }
            Self::Paused { from, .. } | Self::Yielded { from } => Some(from),
        }
    }
}
